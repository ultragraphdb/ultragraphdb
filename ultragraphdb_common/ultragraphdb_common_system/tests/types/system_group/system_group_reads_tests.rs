/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the frontend reads of the books besides the registry (`PROTOCOL.md` §13.7 to §13.9, §13.28): Defaults, a Catalog request
//! of `version` 0 naming an epoch, Challenge and Verify; corner cases C3.69 to C3.72 and the laws L3.30 to L3.32.

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_common_system::{DropStep, ReportAsked, SystemHandle};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_protocol_scram::{ScramClient, ScramRelay, ScramVerifier};
use ultragraphdb_protocol_wire::{Declared, DeclaredPredicate, ErrorCode};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use super::{
    SimOwner, TestClock, closed_after, ended, heard, recovered, refused, running, starting, ticking,
};

const NICK: &str = "http://ex.org/nick";
const SCORE: &str = "http://ex.org/score";

/// The iteration count every principal here derives its verifier with, the least a verifier takes.
const ITERATIONS: u32 = 4_096;

/// The group over `storage` holding tenant 0 and tenant 1, and `ann` with the password `pw`.
async fn group_of(storage: &SimStorage, clock: &TestClock) -> TestResult<SimOwner> {
    let start = starting(MetadataConfig::new())?.with_tenants(vec![1]);
    let mut owner = recovered(storage, &start, clock).await?;
    let verifier = ScramVerifier::derive("pw", b"a salt of ann", ITERATIONS)?;
    owner.create_principal_once("ann", verifier).await?;
    Ok(owner)
}

/// Runs `body` over the group of [`group_of`] on this runtime, then ends it.
fn with_group<F, Fut>(seed: u64, body: F) -> TestResult
where
    F: FnOnce(SystemHandle, TestClock) -> Fut,
    Fut: std::future::Future<Output = TestResult>,
{
    let clock = TestClock::new();
    let storage = SimStorage::new(seed);
    Runtime::new()?.block_on(async {
        let owner = group_of(&storage, &clock).await?;
        let (handle, run) = running(owner, &clock);
        body(handle, clock).await?;
        ended(run).await??;
        Ok(())
    })
}

/// C3.70, and the scenario "Defaults lists the recorded predicates in order": a tenant holding no record answers none; one
/// holding records for `score` and `nick` lists `nick` before `score`, at a log position of at least 1.
#[test]
fn test_defaults_lists_the_recorded_predicates_in_order() -> TestResult {
    with_group(301, |handle, _clock| async move {
        let none = heard(handle.defaults(0).await)?;
        assert!(none.predicates().is_empty());
        assert_eq!(none.tenant(), 0);
        assert!(!heard(handle.record_default(0, SCORE.to_string()).await)?);
        assert!(!heard(handle.record_default(0, NICK.to_string()).await)?);
        let recorded = heard(handle.defaults(0).await)?;
        assert_eq!(recorded.predicates(), [NICK.to_string(), SCORE.to_string()]);
        assert!(recorded.position() >= 1, "{}", recorded.position());
        assert!(recorded.position() > none.position(), "two records later");
        assert!(
            heard(handle.defaults(1).await)?.predicates().is_empty(),
            "tenant 1 holds none"
        );
        Ok(())
    })
}

/// C3.69: a Defaults naming a tenant the group does not hold is refused with code 7, `detail` the tenant.
#[test]
fn test_defaults_of_an_unknown_tenant_is_refused() -> TestResult {
    with_group(302, |handle, _clock| async move {
        let refusal = refused(handle.defaults(9).await)?;
        assert_eq!((refusal.code(), refusal.detail()), (ErrorCode::Refused, 9));
        Ok(())
    })
}

/// C3.71, C3.72: a Catalog request of `version` 0 naming a closed epoch answers the latest version in effect at it, leaving out
/// one pending; one naming the epoch above the latest close is refused with Error 1 naming both epochs; `epoch` 0 answers the
/// current version, pending or not; a tenant holding no version, or none in effect at the epoch, is refused with code 7,
/// `detail` 0. The clock moves one close at a time.
#[test]
fn test_a_catalog_request_at_an_epoch_answers_what_a_client_reads_there() -> TestResult {
    with_group(303, |handle, clock| async move {
        let none = refused(handle.catalog_at(1, 0).await)?;
        assert_eq!((none.code(), none.detail()), (ErrorCode::Refused, 0));
        let before = heard(handle.latest_close().await)?;
        let declaring = {
            let handle = handle.clone();
            compio::runtime::spawn(async move {
                handle.declare_version(0, 0, vec![predicate(NICK)]).await
            })
        };
        for _ in 0..POLLS_MAX {
            if handle
                .catalog(0, 0)
                .await
                .is_ok_and(|held| held.version() == 1)
            {
                break;
            }
            compio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        // The close after the version is pending assigns it the epoch after it (`PROTOCOL.md` §11.13, *Its epoch*).
        let first = closed_after(&handle, &clock, before).await?;
        let answered = heard(
            declaring
                .await
                .map_err(|error| TestError::new(format!("{error:?}")))?,
        )?;
        assert_eq!(answered, (1, (first + 1) << 22));
        let dropped = heard(
            handle
                .drop_version(0, DropStep::Predicate(NICK.to_string()))
                .await,
        )?;
        assert_eq!(dropped, 2, "version 2 pending");
        assert_eq!(
            heard(handle.catalog_at(0, 0).await)?.version(),
            2,
            "the current one, pending"
        );
        let early = refused(handle.catalog_at(0, first).await)?;
        assert_eq!(
            (early.code(), early.detail()),
            (ErrorCode::Refused, 0),
            "none in effect at {first}"
        );
        let second = closed_after(&handle, &clock, first).await?;
        assert_eq!(heard(handle.catalog_at(0, second).await)?.version(), 1);
        let ahead = refused(handle.catalog_at(0, second + 1).await)?;
        assert_eq!(ahead.code(), ErrorCode::Protocol);
        assert!(
            ahead.message().contains(&(second + 1).to_string())
                && ahead.message().contains(&second.to_string()),
            "{}",
            ahead.message()
        );
        let third = closed_after(&handle, &clock, second).await?;
        assert_eq!(heard(handle.catalog_at(0, third).await)?.version(), 2);
        assert_eq!(
            heard(handle.catalog_at(0, second).await)?.version(),
            1,
            "a view behind a close"
        );
        Ok(())
    })
}

/// L3.20, C3.61: a version being checked is never what a Catalog request of `version` 0 naming a closed epoch answers, before its
/// check ends or after its withdrawal.
#[test]
fn test_a_catalog_request_at_an_epoch_never_answers_a_version_being_checked() -> TestResult {
    with_group(304, |handle, clock| async move {
        let ticked = ticking(&clock);
        let (base, _) = heard(handle.declare_version(0, 0, vec![predicate(NICK)]).await)?;
        let single = Declared::Predicate(DeclaredPredicate::new(NICK).with_single(true));
        let checked = {
            let handle = handle.clone();
            compio::runtime::spawn(
                async move { handle.declare_version(0, base, vec![single]).await },
            )
        };
        let tightened = loop_until(&handle, |catalog| {
            catalog.version() == base + 1 && catalog.epoch() != 0
        })
        .await?;
        let closed = closed_after(&handle, &clock, tightened).await?;
        assert_eq!(
            heard(handle.catalog_at(0, closed).await)?.version(),
            base,
            "not the one being checked"
        );
        let refusal = ReportAsked::new(
            Tenant::new(0),
            base + 1,
            1,
            Some((NICK.to_string(), "\"x\"".to_string())),
        );
        heard(handle.report(refusal).await)?;
        let told = checked
            .await
            .map_err(|error| TestError::new(format!("{error:?}")))?;
        assert_eq!(refused(told)?.code(), ErrorCode::Refused);
        let restoring = loop_until(&handle, |catalog| {
            catalog.version() == base + 2 && catalog.epoch() != 0
        })
        .await?;
        let after = closed_after(&handle, &clock, restoring).await?;
        drop(ticked);
        assert_eq!(
            heard(handle.catalog_at(0, after).await)?.version(),
            base + 2
        );
        for epoch in 1..=after {
            if let Ok(read) = handle.catalog_at(0, epoch).await {
                assert_ne!(
                    read.version(),
                    base + 1,
                    "the withdrawn version at epoch {epoch}"
                );
            }
        }
        Ok(())
    })
}

/// L3.31, C3.69 of Challenge: a held name's Challenge answers its salt and count; a name no principal holds answers a salt of its
/// own, the same each time and after a restart, the count the group derives a principal's verifier with, and no two such names
/// share a salt.
#[test]
fn test_a_challenge_answers_a_held_name_and_an_unknown_one_alike() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(305);
    Runtime::new()?.block_on(async {
        let owner = group_of(&storage, &clock).await?;
        let (handle, run) = running(owner, &clock);
        assert_eq!(
            heard(handle.challenge("ann".to_string()).await)?,
            (ITERATIONS, b"a salt of ann".to_vec())
        );
        let nobody = heard(handle.challenge("nobody".to_string()).await)?;
        let someone = heard(handle.challenge("someone".to_string()).await)?;
        // The count `ultragraphdb_protocol_scram::DEFAULT_ITERATIONS` names, the bootstrap administrator's.
        assert_eq!(nobody.0, 600_000);
        assert_ne!(nobody.1, someone.1, "two unknown names, two salts");
        assert_eq!(
            heard(handle.challenge("nobody".to_string()).await)?,
            nobody,
            "one name, one salt"
        );
        drop(handle);
        ended(run).await??;
        let restarted = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(restarted, &clock);
        assert_eq!(
            heard(handle.challenge("nobody".to_string()).await)?,
            nobody,
            "the same after a restart"
        );
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// A client's exchange as `name` of `password`, relayed against the group's Challenge: the client, the relay, and the
/// AuthMessage and ClientProof a Verify carries.
async fn exchange(
    handle: &SystemHandle,
    name: &str,
    password: &str,
) -> TestResult<(ScramClient, ScramRelay, String, [u8; 32])> {
    let mut client = ScramClient::new(name, password, "client-nonce")?;
    let mut relay = ScramRelay::new("group-nonce");
    let asked = relay.first(&client.first_message()?)?;
    let (iterations, salt) = heard(handle.challenge(asked).await)?;
    let server_first = relay.challenge(&salt, iterations)?;
    let client_final = client.final_message(&server_first)?;
    let (auth_message, proof) = relay.last(&client_final)?;
    Ok((client, relay, auth_message, proof))
}

/// L3.32, and the scenarios "A proof that holds is answered with its signature" and "An unknown name and a wrong proof answer
/// alike": a Verify of `ann`'s proof answers her principal and a ServerSignature of 32 bytes; a wrong proof and any proof for an
/// unknown name answer nothing, alike.
#[test]
fn test_a_verify_answers_a_proof_that_holds_alone() -> TestResult {
    with_group(306, |handle, _clock| async move {
        let ann = super::principal(&handle, "ann").await?;
        let (mut client, mut relay, auth_message, proof) = exchange(&handle, "ann", "pw").await?;
        let verified = heard(
            handle
                .verify("ann".to_string(), auth_message.clone(), proof)
                .await,
        )?;
        let (principal, signature) = verified.context("verified")?;
        assert_eq!(principal, ann);
        // The client checks the ServerSignature by RFC 5802 from the password alone.
        client.verify(&relay.finish(&signature)?)?;
        let mut wrong = proof;
        if let Some(first) = wrong.first_mut() {
            *first ^= 1;
        }
        assert_eq!(
            heard(
                handle
                    .verify("ann".to_string(), auth_message.clone(), wrong)
                    .await
            )?,
            None
        );
        assert_eq!(
            heard(
                handle
                    .verify("nobody".to_string(), auth_message, proof)
                    .await
            )?,
            None
        );
        Ok(())
    })
}

/// The scenario "No frame carries a stored key": what a Challenge and a Verify of `ann` answer holds neither her stored key nor
/// her server key.
#[test]
fn test_no_answer_carries_a_stored_key() -> TestResult {
    with_group(307, |handle, _clock| async move {
        let held = ScramVerifier::derive("pw", b"a salt of ann", ITERATIONS)?;
        let (iterations, salt) = heard(handle.challenge("ann".to_string()).await)?;
        let (_client, _relay, auth_message, proof) = exchange(&handle, "ann", "pw").await?;
        let (principal, signature) =
            heard(handle.verify("ann".to_string(), auth_message, proof).await)?
                .context("verified")?;
        let mut answered = salt;
        answered.extend_from_slice(&iterations.to_le_bytes());
        answered.extend_from_slice(&principal.to_le_bytes());
        answered.extend_from_slice(&signature);
        for key in [held.stored_key(), held.server_key()] {
            assert!(
                !answered.windows(32).any(|run| run == key),
                "a key left the group"
            );
        }
        Ok(())
    })
}

/// A declaration of `iri`, its defaults.
fn predicate(iri: &str) -> Declared {
    Declared::Predicate(DeclaredPredicate::new(iri))
}

/// How many millisecond polls a wait takes before it fails.
const POLLS_MAX: u32 = 5_000;

/// Tenant 0's current version once `holds` of it, read each millisecond: its effective epoch.
async fn loop_until(
    handle: &SystemHandle,
    holds: impl Fn(&ultragraphdb_protocol_wire::Catalog) -> bool,
) -> TestResult<u64> {
    for _ in 0..POLLS_MAX {
        let catalog = heard(handle.catalog(0, 0).await)?;
        if holds(&catalog) {
            return Ok(catalog.epoch());
        }
        compio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
    Err(TestError::new("the catalog never came to hold"))
}
