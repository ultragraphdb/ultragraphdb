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
use ultragraphdb_common_frontend::{Owner, owner_run};
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
        let mut owner = Owner::recover(meta_log, seq_log, &start, None, clock.now()).await?;
        let verifier = ScramVerifier::derive("pw", b"salt", 4_096)?;
        owner.create_principal_once("ann", verifier).await?;
        let (handle, run) = owner_run(owner, clock.reading());
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
