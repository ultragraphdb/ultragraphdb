/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::pin::pin;
use std::time::Duration;

use compio::runtime::Runtime;
use futures::future::poll_immediate;
use oxrdf::NamedNode;
use ultragraphdb_cluster_metadata::{MetadataCommand, MetadataConfig, MetadataState};
use ultragraphdb_common_system::{DropStep, ReportAsked, SystemHandle};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_protocol_wire::{
    Declared, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation, Describe,
    ErrorCode,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use super::{
    TestClock, closed_until, closing, ended, heard, recovered, refused, running, sent, starting,
    turned,
};

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

/// Partition 1's report that its data bears out the version of `tenant` above `after`, sent once a close has assigned that
/// version's epoch.
async fn pass(handle: &SystemHandle, clock: &TestClock, tenant: u64, after: u64) -> TestResult {
    let version = closed_until(handle, clock, || async {
        Ok(handle
            .catalog(tenant, 0)
            .await
            .ok()
            .filter(|held| held.version() > after && held.epoch() != 0)
            .map(|held| held.version()))
    })
    .await?;
    let report = ReportAsked::new(Tenant::new(tenant), version, 1, None);
    heard(handle.report(report).await)
}

/// `declared` committed on `built_on`, partition 1 bearing it out: the version's epoch, once it is in effect at a close.
async fn settled(
    handle: &SystemHandle,
    clock: &TestClock,
    tenant: u64,
    built_on: u64,
    declared: Vec<Declared>,
) -> TestResult<u64> {
    let mut answer = pin!(handle.declare(tenant, built_on, declared));
    sent(answer.as_mut()).await?;
    pass(handle, clock, tenant, built_on).await?;
    let epoch = heard(closing(handle, clock, answer).await?)? >> VERSION_BITS;
    let _closed = heard(closing(handle, clock, handle.next_close(epoch)).await?)?;
    Ok(epoch)
}

/// What Describe says of tenant 0.
async fn described(handle: &SystemHandle) -> TestResult<Describe> {
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
        let far = 99;
        let stale = refused(handle.declare(0, far, vec![predicate(P)]).await)?;
        assert_eq!(stale.code(), ErrorCode::Conflict);
        assert!(
            stale.message().contains(&format!("built on {far}")),
            "{}",
            stale.message()
        );
        // Both offers are sent before the owner's run is polled again, so one turn takes them in this order.
        let (first, second) = closing(
            &handle,
            &clock,
            futures::future::join(
                handle.declare(0, 0, vec![predicate(P)]),
                handle.declare(0, 0, vec![predicate(Q)]),
            ),
        )
        .await?;
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
        let tenants = 1..=(MetadataState::PENDING_MAX as u64 + 1);
        let start = starting(MetadataConfig::new())?.with_tenants(tenants.clone().collect());
        let owner = recovered(&storage, &start, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let answers = closing(
            &handle,
            &clock,
            futures::future::join_all(
                tenants.map(|tenant| handle.declare(tenant, 0, vec![predicate(P)])),
            ),
        )
        .await?;
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
        let animal = Declared::EntityType(DeclaredEntityType::new(ANIMAL, None, false));
        let _effective = settled(&handle, &clock, 0, 0, vec![animal, predicate(P)]).await?;
        heard(
            handle
                .drop_catalog(0, DropStep::Predicate(P.to_string()))
                .await,
        )?;
        let after_predicate = heard(handle.catalog(0, 0).await)?.version();
        let described = loop_until_described(&handle, &clock, |describe| {
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

/// Describe of tenant 0 once `holds` of it, read again after each close.
async fn loop_until_described(
    handle: &SystemHandle,
    clock: &TestClock,
    holds: impl Fn(&Describe) -> bool,
) -> TestResult<Describe> {
    closed_until(handle, clock, || async {
        Ok(Some(described(handle).await?).filter(|describe| holds(describe)))
    })
    .await
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
        let not_iri = "not an IRI";
        let refusal = refused(handle.declare(0, 0, vec![predicate(not_iri)]).await)?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        assert!(refusal.message().contains(not_iri), "{}", refusal.message());
        let options = vec![("m".to_string(), "16".to_string())];
        let _effective = settled(&handle, &clock, 0, 0, every_kind(&options)).await?;
        let version = heard(handle.catalog(0, 0).await)?.version();
        let replaced = vec![Declared::Relation(DeclaredRelation::new(
            OWNS,
            vec![OWNER_ROLE.to_string()],
        ))];
        let _replaced = settled(&handle, &clock, 0, version, replaced).await?;
        let describe = loop_until_described(&handle, &clock, |describe| {
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

/// C3.56, C3.57, C3.58, L3.21: a tenant holding no version takes version 1 built on 0, answered at the first close after it is
/// pending with that close's epoch plus one as its commit version; with version 3 current, a version built on 2, on 4 or on 0
/// is refused as a Catalog conflict naming both versions, and version 3 stays current.
#[test]
fn test_a_declare_builds_on_the_current_version_alone() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(31);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let before = heard(handle.latest_close().await)?;
        let declaring = {
            let handle = handle.clone();
            compio::runtime::spawn(
                async move { handle.declare_version(0, 0, vec![predicate(P)]).await },
            )
        };
        loop_until_version(&handle, 1).await?;
        let close = super::closed_after(&handle, &clock, before).await?;
        let answered = heard(
            declaring
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        assert_eq!(answered, (1, (close + 1) << VERSION_BITS));
        let (two, _) = heard(
            closing(
                &handle,
                &clock,
                handle.declare_version(0, 1, vec![predicate(Q)]),
            )
            .await?,
        )?;
        let (three, _) = heard(
            closing(
                &handle,
                &clock,
                handle.declare_version(0, two, vec![predicate(EDGE)]),
            )
            .await?,
        )?;
        assert_eq!((two, three), (2, 3), "numbered without gap");
        for stale in [2, 4, 0] {
            let refusal = refused(handle.declare_version(0, stale, vec![predicate(P)]).await)?;
            assert_eq!(
                (refusal.code(), refusal.detail()),
                (ErrorCode::Conflict, 5),
                "built on {stale}"
            );
            assert!(
                refusal.message().contains(&stale.to_string()) && refusal.message().contains('3'),
                "{}",
                refusal.message()
            );
        }
        assert_eq!(
            heard(handle.catalog(0, 0).await)?.version(),
            3,
            "version 3 stays current"
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// C3.59: a version built on one still pending is committed after it, and both take their epoch at the next close.
#[test]
fn test_a_version_built_on_a_pending_one_takes_the_same_close() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(32);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let declaring = |built_on: u64, iri: &'static str| {
            let handle = handle.clone();
            compio::runtime::spawn(async move {
                handle
                    .declare_version(0, built_on, vec![predicate(iri)])
                    .await
            })
        };
        let first = declaring(0, P);
        loop_until_version(&handle, 1).await?;
        let second = declaring(1, Q);
        loop_until_version(&handle, 2).await?;
        clock.advance(super::EPOCH_US);
        let first = heard(
            first
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        let second = heard(
            second
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        assert_eq!((first.0, second.0), (1, 2));
        assert_eq!(first.1, second.1, "one close assigned both");
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// Tenant 0's current version, once it is `version`, read each millisecond.
async fn loop_until_version(handle: &SystemHandle, version: u64) -> TestResult {
    for _ in 0..POLLS_MAX {
        if handle
            .catalog(0, 0)
            .await
            .is_ok_and(|held| held.version() == version)
        {
            return Ok(());
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!(
        "version {version} was never current"
    )))
}

/// C3.62, C3.63, L3.25: a Declare whose asker gave up before its close takes its epoch all the same, and one of a Drop whose
/// asker gave up is pending and durable: the group's state is what it would be had the asker stayed.
#[test]
fn test_a_declare_whose_asker_left_changes_nothing() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(33);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let gone = handle.declare_version(0, 0, vec![predicate(P)]);
        assert!(
            compio::time::timeout(Duration::from_millis(5), gone)
                .await
                .is_err(),
            "no close yet"
        );
        clock.advance(super::EPOCH_US);
        let effective = loop_until_epoch(&handle, 1).await?;
        assert!(effective > 0, "version 1 took its epoch");
        // Polled once, the ask is sent and its future dropped before any answer: the asker left.
        let dropping = handle.drop_version(0, DropStep::Predicate(P.to_string()));
        assert!(
            futures::future::poll_immediate(dropping).await.is_none(),
            "the asker left at once"
        );
        loop_until_version(&handle, 2).await?;
        drop(handle);
        ended(run).await??;
        let restarted = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(restarted, &clock);
        assert_eq!(
            heard(handle.catalog(0, 0).await)?.version(),
            2,
            "version 2 durable"
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// The effective epoch of tenant 0's `version`, `None` while it is unassigned.
async fn epoch_of(handle: &SystemHandle, version: u64) -> TestResult<Option<u64>> {
    Ok(handle
        .catalog(0, version)
        .await
        .ok()
        .map(|held| held.epoch())
        .filter(|epoch| *epoch != 0))
}

/// The effective epoch of tenant 0's `version`, once assigned, read each millisecond.
async fn loop_until_epoch(handle: &SystemHandle, version: u64) -> TestResult<u64> {
    for _ in 0..POLLS_MAX {
        if let Some(epoch) = epoch_of(handle, version).await? {
            return Ok(epoch);
        }
        compio::time::sleep(Duration::from_millis(1)).await;
    }
    Err(TestError::new(format!(
        "version {version} was never assigned its epoch"
    )))
}

/// C3.65, C3.66, L3.24: a Declare of `kind` 2 for a Drop of a predicate the catalog does not declare, of a graph or of a
/// tenant's data answers `version` 0 at once and leaves the current version.
#[test]
fn test_a_drop_that_changes_nothing_builds_no_version() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(34);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let (one, _) = heard(
            closing(
                &handle,
                &clock,
                handle.declare_version(0, 0, vec![predicate(P)]),
            )
            .await?,
        )?;
        for step in [DropStep::Predicate(Q.to_string()), DropStep::Kept] {
            assert_eq!(
                heard(handle.drop_version(0, step.clone()).await)?,
                0,
                "{step:?}"
            );
        }
        assert_eq!(heard(handle.catalog(0, 0).await)?.version(), one);
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// C3.67, L3.26: a Drop of everything removes the tenant's default-entry records, with a version of the initial catalog when the
/// tenant held a declaration, and alone, `version` 0, when it held the initial catalog; a Drop of a predicate keeps them.
#[test]
fn test_a_drop_of_everything_removes_the_default_entries() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(35);
    Runtime::new()?.block_on(async {
        let start = starting(MetadataConfig::new())?.with_tenants(vec![1]);
        let owner = recovered(&storage, &start, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let (one, _) = heard(
            closing(
                &handle,
                &clock,
                handle.declare_version(0, 0, vec![predicate(P), predicate(Q)]),
            )
            .await?,
        )?;
        for tenant in [0, 1] {
            assert!(!heard(
                handle.record_default(tenant, EDGE.to_string()).await
            )?);
        }
        assert_eq!(
            heard(
                handle
                    .drop_version(0, DropStep::Predicate(P.to_string()))
                    .await
            )?,
            one + 1
        );
        assert_eq!(
            heard(handle.defaults(0).await)?.predicates(),
            [EDGE.to_string()],
            "a predicate's drop keeps them"
        );
        assert_eq!(
            heard(handle.drop_version(0, DropStep::Everything).await)?,
            one + 2
        );
        assert!(heard(handle.defaults(0).await)?.predicates().is_empty());
        assert_eq!(
            heard(handle.drop_version(1, DropStep::Everything).await)?,
            0,
            "the initial catalog already"
        );
        assert!(heard(handle.defaults(1).await)?.predicates().is_empty());
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// C3.69, L3.28: a Declare of either kind naming a tenant the group does not hold is refused with code 7, `detail` the tenant,
/// and changes nothing.
#[test]
fn test_a_declare_of_an_unknown_tenant_is_refused() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(36);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let declared = refused(handle.declare_version(9, 0, vec![predicate(P)]).await)?;
        assert_eq!(
            (declared.code(), declared.detail()),
            (ErrorCode::Refused, 9)
        );
        let dropped = refused(handle.drop_version(9, DropStep::Everything).await)?;
        assert_eq!((dropped.code(), dropped.detail()), (ErrorCode::Refused, 9));
        assert_eq!(
            heard(handle.catalog(0, 0).await)
                .map(|held| held.version())
                .unwrap_or(0),
            0
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// C3.68: a tightening checked over two partitions waits for both: one report passed leaves it waiting, and the other silent is
/// still waited for one tick before `catalog_check_ms` and withdraws it at `catalog_check_ms`, its Commit refused with code 24
/// naming the partition.
#[test]
fn test_a_silent_partition_withdraws_a_tightening_at_the_wait() -> TestResult {
    const CHECK_MS: u64 = 40;
    let clock = TestClock::new();
    let storage = SimStorage::new(37);
    Runtime::new()?.block_on(async {
        let metadata = MetadataConfig::new().with_catalog_check_ms(CHECK_MS)?;
        let start = ultragraphdb_common_system::SystemStart::new(
            metadata,
            ultragraphdb_cluster_sequencer::SequencerConfig::new(1, 5)?,
            2,
        );
        let owner = recovered(&storage, &start, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let (one, _) = heard(
            closing(
                &handle,
                &clock,
                handle.declare_version(0, 0, vec![predicate(P)]),
            )
            .await?,
        )?;
        let single = Declared::Predicate(DeclaredPredicate::new(P).with_single(true));
        let told = {
            let mut checked = pin!(handle.declare_version(0, one, vec![single]));
            sent(checked.as_mut()).await?;
            // The clock stays where the close assigning the epoch left it: every turn of the group from that close on reads
            // it, so the first to see the epoch assigned begins the wait there.
            let _epoch = closed_until(&handle, &clock, || epoch_of(&handle, one + 1)).await?;
            heard(
                handle
                    .report(ReportAsked::new(Tenant::new(0), one + 1, 1, None))
                    .await,
            )?;
            clock.advance(CHECK_MS * 1_000 - 1);
            // A turn has run its checks at this reading and ended, and a withdrawal would have told the Commit in it.
            let _latest = turned(&handle).await?;
            assert!(
                poll_immediate(checked.as_mut()).await.is_none(),
                "still waiting one tick before the wait ends"
            );
            clock.advance(1);
            let _latest = heard(handle.latest_close().await)?;
            checked.await
        };
        let refusal = refused(told)?;
        assert_eq!((refusal.code(), refusal.detail()), (ErrorCode::TimedOut, 2));
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// The declarations version `version` of tenant 0 holds, in their order.
async fn held_in(handle: &SystemHandle, version: u64) -> TestResult<Vec<Declaration>> {
    let catalog = heard(handle.catalog(0, version).await)?;
    Ok(CatalogContents::decode(0, version, catalog.bytes())?.into_declarations())
}

/// A predicate declared with nothing but its IRI.
fn declared_plain(iri: &str) -> TestResult<Declaration> {
    Ok(Declaration::Predicate(PredicateDeclaration::new(
        NamedNode::new(iri)?,
    )))
}

/// A Declare's declarations land in order: one of a name held replaces it where it stands, one of a new name is appended, and a
/// later one of the same name replaces the earlier in its place.
#[test]
fn test_declarations_replace_what_they_name_in_place() -> TestResult {
    const R: &str = "http://ex.org/r";
    let clock = TestClock::new();
    let storage = SimStorage::new(38);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let (one, _) = heard(
            closing(
                &handle,
                &clock,
                handle.declare_version(0, 0, vec![predicate(P), predicate(Q)]),
            )
            .await?,
        )?;
        // Uniqueness and a count index neither tighten nor loosen, so no check holds the version.
        let unique_p = DeclaredPredicate::new(P).with_unique(true);
        let counted_r = DeclaredPredicate::new(R).with_count_index(true);
        let declared = vec![
            predicate(R),
            Declared::Predicate(unique_p),
            Declared::Predicate(counted_r),
        ];
        let (two, _) =
            heard(closing(&handle, &clock, handle.declare_version(0, one, declared)).await?)?;
        let expected = vec![
            Declaration::Predicate(PredicateDeclaration::new(NamedNode::new(P)?).with_unique(true)),
            declared_plain(Q)?,
            Declaration::Predicate(
                PredicateDeclaration::new(NamedNode::new(R)?).with_count_index(true),
            ),
        ];
        assert_eq!(held_in(&handle, two).await?, expected);
        assert_eq!(
            held_in(&handle, one).await?,
            vec![declared_plain(P)?, declared_plain(Q)?],
            "the version built on stays as it was"
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// A Declare's refusal names its predicates first: the first predicate naming no IRI, though a type before it names none; then
/// a tightening with a loosening, though a relation after them names no IRI; and only then a type or relation naming none.
#[test]
fn test_a_declare_is_refused_for_its_predicates_first() -> TestResult {
    const NOT_A_TYPE: &str = "not a type";
    const NOT_A_PREDICATE: &str = "not a predicate";
    const NOT_A_RELATION: &str = "not a relation";
    let clock = TestClock::new();
    let storage = SimStorage::new(39);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let bad_type = Declared::EntityType(DeclaredEntityType::new(NOT_A_TYPE, None, false));
        let bad_relation = Declared::Relation(DeclaredRelation::new(NOT_A_RELATION, Vec::new()));
        let refusal = refused(
            handle
                .declare(0, 0, vec![bad_type.clone(), predicate(NOT_A_PREDICATE)])
                .await,
        )?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        assert!(
            refusal.message().contains(NOT_A_PREDICATE),
            "{}",
            refusal.message()
        );
        let single_q = Declared::Predicate(DeclaredPredicate::new(Q).with_single(true));
        let _effective = settled(&handle, &clock, 0, 0, vec![single_q]).await?;
        let version = heard(handle.catalog(0, 0).await)?.version();
        let single_p = Declared::Predicate(DeclaredPredicate::new(P).with_single(true));
        let declared = vec![single_p, predicate(Q), bad_relation.clone()];
        let refusal = refused(handle.declare(0, version, declared).await)?;
        assert_eq!((refusal.code(), refusal.detail()), (ErrorCode::Refused, 1));
        assert!(refusal.message().contains(Q), "{}", refusal.message());
        let refusal = refused(
            handle
                .declare(0, version, vec![bad_relation, predicate(P), bad_type])
                .await,
        )?;
        assert_eq!(refusal.code(), ErrorCode::Refused);
        assert!(
            refusal.message().contains(NOT_A_RELATION),
            "{}",
            refusal.message()
        );
        assert_eq!(
            heard(handle.catalog(0, 0).await)?.version(),
            version,
            "nothing committed"
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}
