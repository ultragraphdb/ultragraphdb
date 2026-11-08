/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use compio::runtime::Runtime;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_common_frontend::Owner;
use ultragraphdb_error_role::RoleErrorEnum;
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transactions_log::{FaultMode, SimStorage, Wal, WalConfig};

use super::{CLUSTER, SimOwner, TestClock, closed_after, ended, recovered, running, starting};

/// How many closes a test waits for the sequencer's snapshot: past the closes between two.
const CLOSES_MAX: u64 = 10_000;

const SEQUENCER_SNAPSHOT: &str = "sequencer.snapshot";

/// A directory of this test process's own.
fn directory(name: &str) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let directory = root.join(format!("owner-turn-{name}-{}", std::process::id()));
    if directory.exists() {
        std::fs::remove_dir_all(&directory)?;
    }
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

/// The owner over the logs `storage` holds, its snapshots in `snapshots`.
async fn with_snapshots(
    storage: &SimStorage,
    snapshots: PathBuf,
    clock: &TestClock,
) -> TestResult<SimOwner> {
    let (metadata, _) = Wal::open(storage.clone(), "/meta", WalConfig::new(CLUSTER, false)).await?;
    let (sequencer, _) = Wal::open(storage.clone(), "/seq", WalConfig::new(CLUSTER, false)).await?;
    let start = starting(MetadataConfig::new())?;
    Ok(Owner::recover(metadata, sequencer, &start, Some(snapshots), clock.now()).await?)
}

/// The sequencer stores a snapshot after so many closes and trims its log through it, and a restart recovers from it: the closes
/// go on after the last.
#[test]
fn test_the_sequencer_snapshots_every_so_many_closes() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(11);
    let snapshots = directory("sequencer")?;
    Runtime::new()?.block_on(async {
        let owner = with_snapshots(&storage, snapshots.clone(), &clock).await?;
        let (handle, run) = running(owner, &clock);
        let mut last = 0;
        while !snapshots.join(SEQUENCER_SNAPSHOT).exists() {
            if last >= CLOSES_MAX {
                return Err(TestError::new(format!("no snapshot after {last} closes")));
            }
            last = closed_after(&handle, &clock, last).await?;
        }
        drop(handle);
        ended(run).await??;
        let restarted = with_snapshots(&storage, snapshots.clone(), &clock).await?;
        let (again, rerun) = running(restarted, &clock);
        assert!(closed_after(&again, &clock, last).await? > last);
        drop(again);
        ended(rerun).await??;
        Ok(())
    })
}

/// A sync of the sequencer's log that fails stops the owner with the code of an I/O failure.
#[test]
fn test_a_failed_sync_of_the_sequencers_log_stops_the_owner() -> TestResult {
    let clock = TestClock::new();
    let storage = SimStorage::new(12);
    Runtime::new()?.block_on(async {
        let owner = recovered(&storage, &starting(MetadataConfig::new())?, &clock).await?;
        let (handle, run) = running(owner, &clock);
        let first = closed_after(&handle, &clock, 0).await?;
        // The next operation is the write of the next close's round.
        storage.fail_at(storage.trace().len(), FaultMode::Before);
        clock.advance(super::EPOCH_US);
        let stopped = handle.next_close(first).await;
        assert!(stopped.is_err(), "no close after the failure");
        drop(handle);
        let error = ended(run)
            .await?
            .err()
            .ok_or_else(|| TestError::new("the owner went on"))?;
        assert!(matches!(error.0, RoleErrorEnum::Io { .. }), "{error}");
        Ok(())
    })
}
