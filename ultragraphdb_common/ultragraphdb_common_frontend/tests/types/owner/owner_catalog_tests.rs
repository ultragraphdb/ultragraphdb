/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataConfig, MetadataState};
use ultragraphdb_common_frontend::{DropStep, OwnerHandle, ReportAsked};
use ultragraphdb_protocol_wire::{
    Declared, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation, Describe,
    ErrorCode,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use super::{TestClock, ended, heard, recovered, refused, running, starting, ticking};

const P: &str = "http://ex.org/p";
const Q: &str = "http://ex.org/q";
const EDGE: &str = "http://ex.org/knows";
const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const ANIMAL: &str = "http://ex.org/Animal";
const DOG: &str = "http://ex.org/Dog";
const OWNS: &str = "http://ex.org/Owns";
const OWNER_ROLE: &str = "http://ex.org/owner";
const PET_ROLE: &str = "http://ex.org/pet";
const FACTORY: &str = "hnsw";
const VERSION_BITS: u32 = 22;

/// How many millisecond polls a wait on the owner takes before it fails.
const POLLS_MAX: u32 = 5_000;

/// Partition 1's report that its data bears out the version of `tenant` above `after`, sent once that version's epoch is assigned.
async fn pass(handle: &OwnerHandle, tenant: u64, after: u64) -> TestResult {
    for _ in 0..POLLS_MAX {
        if let Ok(held) = handle.catalog(tenant, 0).await
            && held.version() > after
            && held.epoch() != 0
        {
            let report = ReportAsked {
                tenant,
                version: held.version(),
                partition: 1,
                refused: None,
            };
            return heard(handle.report(report).await);
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!(
        "no version above {after} was assigned its epoch"
    )))
}

/// `declared` committed on `built_on`, partition 1 bearing it out: the version's epoch, once it is in effect at a close.
async fn settled(
    handle: &OwnerHandle,
    tenant: u64,
    built_on: u64,
    declared: Vec<Declared>,
) -> TestResult<u64> {
    let (answer, passed) = futures::join!(
        handle.declare(tenant, built_on, declared),
        pass(handle, tenant, built_on)
    );
    passed?;
    let epoch = heard(answer)? >> VERSION_BITS;
    let _closed = heard(handle.next_close(epoch).await)?;
    Ok(epoch)
}

/// What Describe says of tenant 0.
async fn described(handle: &OwnerHandle) -> TestResult<Describe> {
    heard(handle.describe(0).await)
}

/// A version built on one not current is refused as a Catalog conflict naming both, whether the version it is built on was replaced
/// before or by an offer taken earlier in the same turn.
#[test]
fn test_a_version_built_on_one_not_current_is_stale() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(3);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let _ticking = ticking(&clock);
        let far = 99;
        let stale = refused(handle.declare(0, far, vec![predicate(P)]).await)?;
        assert_eq!(stale.code(), ErrorCode::Conflict);
        assert!(
            stale.message().contains(&format!("built on {far}")),
            "{}",
            stale.message()
        );
        // Both offers are sent before the owner's run is polled again, so one turn takes them in this order.
        let (first, second) = futures::join!(
            handle.declare(0, 0, vec![predicate(P)]),
            handle.declare(0, 0, vec![predicate(Q)])
        );
        assert!(heard(first)? > 0);
        let behind = refused(second)?;
        assert_eq!(
            (behind.code(), behind.detail()),
            (ErrorCode::Conflict, stale.detail())
        );
        assert!(
            behind.message().contains("1 is current"),
            "{}",
            behind.message()
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

fn predicate(iri: &str) -> Declared {
    Declared::Predicate(DeclaredPredicate::new(iri))
}

/// An offer past the pending versions the group holds is refused with code 7 naming them, and the versions before it stand.
#[test]
fn test_an_offer_past_the_pending_versions_held_is_refused() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(4);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let _ticking = ticking(&clock);
        let tenants = 1..=(MetadataState::PENDING_MAX as u64 + 1);
        let answers = futures::future::join_all(
            tenants.map(|tenant| handle.declare(tenant, 0, vec![predicate(P)])),
        )
        .await;
        let (last, held) = answers.split_last().context("the answers")?;
        assert!(
            held.iter().all(Result::is_ok),
            "every version before the last stands"
        );
        let full = refused(last.clone())?;
        assert_eq!(full.code(), ErrorCode::Refused);
        assert!(
            full.message()
                .contains(&MetadataState::PENDING_MAX.to_string()),
            "{}",
            full.message()
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// A Drop of everything builds the initial catalog; a second taken in the same turn is built on the version the first replaced, and
/// is refused with code 7. A Drop of a predicate leaves an entity type held beside it.
#[test]
fn test_drops_leave_what_they_do_not_name() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(5);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let _ticking = ticking(&clock);
        let animal = Declared::EntityType(DeclaredEntityType::new(ANIMAL, None, false));
        let _effective = settled(&handle, 0, 0, vec![animal, predicate(P)]).await?;
        heard(
            handle
                .drop_catalog(0, DropStep::Predicate(P.to_string()))
                .await,
        )?;
        let after_predicate = heard(handle.catalog(0, 0).await)?.version();
        let described = loop_until_described(&handle, |describe| {
            describe.predicates().is_empty() && describe.entity_types().len() == 1
        })
        .await?;
        assert_eq!(
            described.entity_types().first().map(|(held, _)| held.iri()),
            Some(ANIMAL)
        );
        let (everything, again) = futures::join!(
            handle.drop_catalog(0, DropStep::Everything),
            handle.drop_catalog(0, DropStep::Everything)
        );
        heard(everything)?;
        let stale = refused(again)?;
        assert_eq!(stale.code(), ErrorCode::Refused);
        assert!(
            stale.message().contains(&after_predicate.to_string()),
            "{}",
            stale.message()
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// Describe of tenant 0 once `holds` of it, read each millisecond.
async fn loop_until_described(
    handle: &OwnerHandle,
    holds: impl Fn(&Describe) -> bool,
) -> TestResult<Describe> {
    for _ in 0..POLLS_MAX {
        let describe = described(handle).await?;
        if holds(&describe) {
            return Ok(describe);
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new("Describe never came to hold"))
}

/// A default entry is recorded once: a second record of it, in the same turn or later, finds it held. A predicate past what a
/// record holds is refused with code 7.
#[test]
fn test_a_default_entry_is_recorded_once() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(6);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let (first, second) = futures::join!(
            handle.record_default(0, P.to_string()),
            handle.record_default(0, P.to_string())
        );
        assert_eq!((heard(first)?, heard(second)?), (false, true));
        assert!(heard(handle.record_default(0, P.to_string()).await)?);
        let long = "p".repeat(MetadataCommand::PREDICATE_BYTES_MAX + 1);
        let refusal = refused(handle.record_default(0, long).await)?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// One declaration of every kind: a child entity type before its parent, a relation with two roles, an edge predicate,
/// and a typed predicate with a vector index taking `options`.
fn every_kind(options: &[(String, String)]) -> Vec<Declared> {
    vec![
        Declared::EntityType(DeclaredEntityType::new(
            DOG,
            Some(ANIMAL.to_string()),
            false,
        )),
        Declared::EntityType(DeclaredEntityType::new(ANIMAL, None, true)),
        Declared::Relation(DeclaredRelation::new(
            OWNS,
            vec![OWNER_ROLE.to_string(), PET_ROLE.to_string()],
        )),
        Declared::Predicate(DeclaredPredicate::new(EDGE).with_kind(DeclaredKind::Edge)),
        Declared::Predicate(
            DeclaredPredicate::new(P)
                .with_datatype(XSD_STRING)
                .with_vector_index(FACTORY, options.to_vec()),
        ),
    ]
}

/// Every kind of declaration reaches Describe as declared: a predicate's kind, datatype and vector index, entity types numbered in
/// preorder below their parents, a relation and its roles; a declaration of a name held replaces it. A declaration naming no IRI
/// is refused with code 7 naming it.
#[test]
fn test_every_kind_of_declaration_reaches_describe() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(7);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let _ticking = ticking(&clock);
        let not_iri = "not an IRI";
        let refusal = refused(handle.declare(0, 0, vec![predicate(not_iri)]).await)?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        assert!(refusal.message().contains(not_iri), "{}", refusal.message());
        let options = vec![("m".to_string(), "16".to_string())];
        let _effective = settled(&handle, 0, 0, every_kind(&options)).await?;
        let version = heard(handle.catalog(0, 0).await)?.version();
        let replaced = vec![Declared::Relation(DeclaredRelation::new(
            OWNS,
            vec![OWNER_ROLE.to_string()],
        ))];
        let _replaced = settled(&handle, 0, version, replaced).await?;
        let describe = loop_until_described(&handle, |describe| {
            describe
                .relations()
                .first()
                .is_some_and(|relation| relation.roles().len() == 1)
        })
        .await?;
        let numbered: Vec<(&str, u32)> = describe
            .entity_types()
            .iter()
            .map(|(held, number)| (held.iri(), *number))
            .collect();
        let animal = numbered
            .iter()
            .find(|(iri, _)| *iri == ANIMAL)
            .context("Animal")?;
        let dog = numbered
            .iter()
            .find(|(iri, _)| *iri == DOG)
            .context("Dog")?;
        assert!(animal.1 < dog.1, "a parent before its child: {numbered:?}");
        let typed = describe
            .predicates()
            .iter()
            .find(|held| held.predicate() == P)
            .context("p")?;
        assert_eq!(typed.datatype(), Some(XSD_STRING));
        assert_eq!(typed.vector_index(), Some(&(FACTORY.to_string(), options)));
        assert!(
            describe
                .relationship_types()
                .iter()
                .any(|edge| edge == EDGE)
        );
        assert!(!describe.property_keys().iter().any(|key| key == EDGE));
        assert_eq!(describe.relations().len(), 1);
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}
