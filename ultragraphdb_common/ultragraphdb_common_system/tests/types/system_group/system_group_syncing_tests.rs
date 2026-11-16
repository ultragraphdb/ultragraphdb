/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The owner answers a commit's path while one of its logs syncs (design D17): the gate's facts and the owner check, from
//! what is durable, while every other ask waits for the sync's end.

use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{Runtime, spawn};
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_common_system::{SystemGroup, system_run};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transactions_log::{GatedStorage, SimStorage, SyncGate, Wal, WalConfig};

use super::{CLUSTER, EPOCH_US, TestClock, ended, heard, principal, starting};

/// How long an ask may take to be answered while a sync is held.
const ANSWERED_WITHIN: Duration = Duration::from_secs(2);
/// How long a test watches an ask that waits for the sync.
const WATCHED: Duration = Duration::from_millis(50);

/// The log whose sync a test holds.
#[derive(Debug, Clone, Copy)]
enum Held {
    Metadata,
    Sequencer,
}

/// `answer`, failing the test when it takes longer than [`ANSWERED_WITHIN`].
async fn within<T>(answer: impl Future<Output = T>) -> TestResult<T> {
    compio::time::timeout(ANSWERED_WITHIN, answer)
        .await
        .map_err(|_| TestError::new("not answered while the sync was held".to_string()))
}

/// The owner over `storage`, the metadata group's syncs at `metadata` and the sequencer's at `sequencer`, the principal
/// `ann` created, run on this runtime; then a close falls due while the `held` log's sync is held, and what is asked then
/// is checked.
fn answered_while_held(held: Held) -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(41);
    let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
    Runtime::new()?.block_on(async {
        let gated = |gate: &SyncGate| GatedStorage::new(storage.clone(), gate.clone());
        let config = WalConfig::new(CLUSTER, false);
        let (meta_log, _) = Wal::open(gated(&metadata), "/meta", config).await?;
        let (seq_log, _) = Wal::open(gated(&sequencer), "/seq", config).await?;
        let start = starting(MetadataConfig::new())?;
        let mut owner = SystemGroup::recover(meta_log, seq_log, &start, None, clock.now()).await?;
        let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
        owner.create_principal_once("ann", verifier).await?;
        let (handle, run) = system_run(owner, clock.reading());
        let run = spawn(run);
        let ann = principal(&handle, "ann").await?;
        heard(handle.register_session(1, ann).await)?;
        let before = heard(handle.tenant_facts(0).await)?.last_closed();
        let gate = match held {
            Held::Metadata => &metadata,
            Held::Sequencer => &sequencer,
        };
        gate.hold();
        clock.advance(EPOCH_US);
        gate.until_waiting().await?;
        // The sync is held: the gate's facts and the owner check are answered, from what is durable.
        let facts = heard(within(handle.tenant_facts(0)).await?)?;
        let durable = match held {
            // The sequencer's close is not durable yet: the facts name the close before it.
            Held::Sequencer => before,
            // The close is durable in the sequencer's log, which the group is learning.
            Held::Metadata => before + 1,
        };
        assert_eq!(facts.last_closed(), durable, "{held:?}");
        heard(within(handle.check_session(1, ann)).await?)?;
        assert!(within(handle.session_held(1)).await?, "{held:?}");
        heard(within(handle.begin_commit(1, ann)).await?)?;
        handle.commit_ended(1);
        // A registration writes the metadata group's log: it waits for the sync's end.
        let registered = Rc::new(Cell::new(false));
        let waiting = {
            let (handle, registered) = (handle.clone(), registered.clone());
            spawn(async move {
                let answer = handle.register_session(2, ann).await;
                registered.set(true);
                answer
            })
        };
        compio::time::sleep(WATCHED).await;
        assert!(
            !registered.get(),
            "{held:?}: a registration answered while the sync was held"
        );
        gate.release();
        heard(
            waiting
                .await
                .map_err(|error| TestError::new(format!("the registration: {error:?}")))?,
        )?;
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

#[test]
fn test_a_commits_path_is_answered_while_the_sequencer_syncs_a_close() -> TestResult {
    answered_while_held(Held::Sequencer)
}

#[test]
fn test_a_commits_path_is_answered_while_the_metadata_group_learns_a_close() -> TestResult {
    answered_while_held(Held::Metadata)
}

/// The group over `storage` whose metadata syncs pass `metadata`'s gate and the sequencer's `sequencer`'s, holding `alice`,
/// `transaction_ms` 50 and a registry of `sessions_max`, run on this runtime past its quiet time: its handle, run and
/// `alice`.
async fn gated_group(
    storage: &SimStorage,
    clock: &TestClock,
    gates: (&SyncGate, &SyncGate),
    sessions_max: u32,
) -> TestResult<(
    ultragraphdb_common_system::SystemHandle,
    compio::runtime::JoinHandle<Result<(), ultragraphdb_error_role::RoleError>>,
    u64,
)> {
    let gated = |gate: &SyncGate| GatedStorage::new(storage.clone(), gate.clone());
    let config = WalConfig::new(CLUSTER, false);
    let (meta_log, _) = Wal::open(gated(gates.0), "/meta", config).await?;
    let (seq_log, _) = Wal::open(gated(gates.1), "/seq", config).await?;
    let start = starting(MetadataConfig::new().with_sessions_max(sessions_max)?)?
        .with_transaction_ms(super::system_group_sessions_tests::T_MS);
    let mut owner = SystemGroup::recover(meta_log, seq_log, &start, None, clock.now()).await?;
    let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
    owner.create_principal_once("alice", verifier).await?;
    let (handle, run) = system_run(owner, clock.reading());
    let run = spawn(run);
    let alice = principal(&handle, "alice").await?;
    clock.advance(super::system_group_sessions_tests::HOLD_US);
    Ok((handle, run, alice))
}

/// C3.50, and the scenario "A check is answered while a close syncs": while the sequencer syncs a close, a Check is answered and
/// a Session waits for the sync's end.
#[test]
fn test_a_check_is_answered_while_a_close_syncs_and_a_session_after() -> TestResult {
    use super::system_group_sessions_tests::{MARKS, connected, outcome};
    let clock = TestClock::new();
    let storage = SimStorage::new(51);
    let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
    Runtime::new()?.block_on(async {
        let (handle, run, alice) =
            gated_group(&storage, &clock, (&metadata, &sequencer), 4).await?;
        heard(handle.session(1, alice).await)?;
        let connection = connected(&handle).await?;
        sequencer.hold();
        clock.advance(EPOCH_US);
        sequencer.until_waiting().await?;
        let registered = Rc::new(Cell::new(false));
        let waiting = {
            let (handle, registered) = (handle.clone(), registered.clone());
            spawn(async move {
                let answer = handle.session(2, alice).await;
                registered.set(true);
                answer
            })
        };
        assert_eq!(
            within(outcome(&handle, connection, 1, alice, MARKS)).await??,
            1
        );
        compio::time::sleep(WATCHED).await;
        assert!(
            !registered.get(),
            "a Session answered while the close synced"
        );
        sequencer.release();
        heard(
            waiting
                .await
                .map_err(|error| TestError::new(format!("the Session: {error:?}")))?,
        )?;
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// C3.51 to C3.53, and the scenario "An eviction appended is seen by the next check": while the metadata log syncs a
/// registration that evicted a session, a Check finds the evicted session not held, outcome 12, and the session registered
/// held, its principal as the registration appended names it (`notes/group-3.md`, *Found wanting*, the open item).
#[test]
fn test_a_check_reads_what_the_registry_holds_from_every_command_appended() -> TestResult {
    use super::system_group_sessions_tests::{ASKS, connected, outcomes};
    let clock = TestClock::new();
    let storage = SimStorage::new(52);
    let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
    Runtime::new()?.block_on(async {
        let (handle, run, alice) =
            gated_group(&storage, &clock, (&metadata, &sequencer), 1).await?;
        heard(handle.session(1, alice).await)?;
        let connection = connected(&handle).await?;
        metadata.hold();
        let waiting = {
            let handle = handle.clone();
            spawn(async move { handle.session(2, alice).await })
        };
        metadata.until_waiting().await?;
        let seen = within(outcomes(
            &handle,
            connection,
            &[(1, alice, ASKS), (2, alice, ASKS)],
        ))
        .await??;
        assert_eq!(
            seen,
            [12, 1],
            "the eviction appended is seen, and the registration appended is held"
        );
        metadata.release();
        heard(
            waiting
                .await
                .map_err(|error| TestError::new(format!("the Session: {error:?}")))?,
        )?;
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// C3.54: what arrives while a sync is held, a Declare of a Drop, a Session and a Lease in that order, is answered after the sync
/// ends, in that order.
#[test]
fn test_what_arrives_while_a_sync_is_held_is_answered_after_it_in_order() -> TestResult {
    use ultragraphdb_common_system::DropStep;
    use ultragraphdb_protocol_wire::{Declared, DeclaredPredicate};
    let clock = TestClock::new();
    let storage = SimStorage::new(53);
    let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
    Runtime::new()?.block_on(async {
        let (handle, run, alice) =
            gated_group(&storage, &clock, (&metadata, &sequencer), 4).await?;
        let predicate = "http://ex.org/p";
        let _version = heard(
            super::closing(
                &handle,
                &clock,
                handle.declare_version(
                    0,
                    0,
                    vec![Declared::Predicate(DeclaredPredicate::new(predicate))],
                ),
            )
            .await?,
        )?;
        sequencer.hold();
        clock.advance(EPOCH_US);
        sequencer.until_waiting().await?;
        let order = Rc::new(std::cell::RefCell::new(Vec::new()));
        let told = |what: &'static str| {
            let order = order.clone();
            move || order.borrow_mut().push(what)
        };
        let (dropped, registered, leased) = (told("declare"), told("session"), told("lease"));
        let tasks = (
            {
                let handle = handle.clone();
                spawn(async move {
                    let answer = handle
                        .drop_version(0, DropStep::Predicate(predicate.to_string()))
                        .await;
                    dropped();
                    answer.map(drop)
                })
            },
            {
                let handle = handle.clone();
                spawn(async move {
                    let answer = handle.session(1, alice).await;
                    registered();
                    answer
                })
            },
            {
                let handle = handle.clone();
                spawn(async move {
                    let answer = handle.lease(1, 1, 8).await;
                    leased();
                    answer.map(drop)
                })
            },
        );
        compio::time::sleep(WATCHED).await;
        assert!(
            order.borrow().is_empty(),
            "answered while the sync was held"
        );
        sequencer.release();
        for task in [tasks.0, tasks.1, tasks.2] {
            heard(
                task.await
                    .map_err(|error| TestError::new(format!("{error:?}")))?,
            )?;
        }
        assert_eq!(*order.borrow(), ["declare", "session", "lease"]);
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}

/// C3.55: a registration appended and not durable when the group is killed was never answered, and after recovery the session
/// is held or absent.
#[test]
fn test_a_registration_lost_with_its_sync_was_never_answered() -> TestResult {
    use super::system_group_sessions_tests::{ASKS, connected, outcome};
    let clock = TestClock::new();
    let storage = SimStorage::new(54);
    let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
    Runtime::new()?.block_on(async {
        let (handle, run, alice) =
            gated_group(&storage, &clock, (&metadata, &sequencer), 4).await?;
        metadata.hold();
        let waiting = {
            let handle = handle.clone();
            spawn(async move { handle.session(1, alice).await })
        };
        metadata.until_waiting().await?;
        drop(run);
        storage.power_loss();
        let answer = waiting
            .await
            .map_err(|error| TestError::new(format!("{error:?}")))?;
        assert!(answer.is_err(), "the Session was answered: {answer:?}");
        drop(handle);
        let start = starting(MetadataConfig::new())?
            .with_transaction_ms(super::system_group_sessions_tests::T_MS);
        let restarted = super::recovered(&storage, &start, &clock).await?;
        let (handle, run) = super::running(restarted, &clock);
        clock.advance(super::system_group_sessions_tests::HOLD_US);
        let connection = connected(&handle).await?;
        let seen = outcome(&handle, connection, 1, alice, ASKS).await?;
        assert!(seen == 1 || seen == 12, "held or absent, not {seen}");
        drop(handle);
        ended(run).await??;
        Ok(())
    })
}
