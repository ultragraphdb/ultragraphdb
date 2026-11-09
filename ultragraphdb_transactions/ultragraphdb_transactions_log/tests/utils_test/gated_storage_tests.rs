/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The gate over a storage: every operation reaches the storage it wraps, and a file's sync waits while the gate is held.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_log::{
    AlignedBuf, DurableFile, DurableStorage, GatedStorage, OpenMode, SimStorage, StorageOperation,
    SyncGate,
};

use crate::utils::helpers::run;

/// How long a test watches a held sync, to see that it does not end.
const WATCHED: Duration = Duration::from_millis(20);

#[test]
fn test_every_operation_reaches_the_storage_it_wraps() -> TestResult {
    run(async {
        let storage = SimStorage::new(1);
        let gated = GatedStorage::new(storage.clone(), SyncGate::default());
        let dir = Path::new("/d");
        gated.create_directory(dir).await.context("created")?;
        let mut file = gated
            .open(&dir.join("a"), OpenMode::Create)
            .await
            .context("opened")?;
        file.write_at(0, AlignedBuf::from_slice(b"abcd"))
            .await
            .context("written")?;
        file.preallocate(4_096).await.context("preallocated")?;
        file.truncate(3).await.context("cut")?;
        file.sync().await.context("synced")?;
        assert_eq!(file.length().await.context("length")?, 3);
        assert_eq!(file.read_at(0, 3).await.context("read")?.as_slice(), b"abc");
        gated
            .rename(&dir.join("a"), &dir.join("b"))
            .await
            .context("renamed")?;
        let _aligned = gated
            .open_aligned(&dir.join("c"), OpenMode::Create)
            .await
            .context("opened aligned")?;
        gated.sync_directory(dir).await.context("synced")?;
        assert_eq!(gated.list(dir).await.context("listed")?, ["b", "c"]);
        gated.remove_file(&dir.join("b")).await.context("removed")?;
        assert_eq!(gated.list(dir).await.context("listed")?, ["c"]);
        // Each call above, in order, reached the storage it wraps.
        let reached: Vec<StorageOperation> = storage
            .trace()
            .into_iter()
            .map(|(operation, _)| operation)
            .collect();
        assert_eq!(
            reached,
            [
                StorageOperation::CreateDirectory,
                StorageOperation::Create,
                StorageOperation::Write,
                StorageOperation::Preallocate,
                StorageOperation::Truncate,
                StorageOperation::FileSync,
                StorageOperation::Length,
                StorageOperation::Read,
                StorageOperation::Rename,
                StorageOperation::Create,
                StorageOperation::DirectorySync,
                StorageOperation::List,
                StorageOperation::Remove,
                StorageOperation::List,
            ]
        );
        Ok::<(), TestError>(())
    })??;
    Ok(())
}

#[test]
fn test_a_sync_waits_while_the_gate_is_held() -> TestResult {
    run(async {
        let gate = SyncGate::default();
        let gated = GatedStorage::new(SimStorage::new(1), gate.clone());
        let file = gated
            .open(Path::new("/a"), OpenMode::Create)
            .await
            .context("opened")?;
        gate.hold();
        let synced = Rc::new(Cell::new(false));
        let syncing = {
            let synced = synced.clone();
            spawn(async move {
                let outcome = file.sync().await;
                synced.set(true);
                outcome.map(|()| file)
            })
        };
        gate.until_waiting().await.context("reached the gate")?;
        compio::time::sleep(WATCHED).await;
        assert!(!synced.get(), "a sync ended while the gate was held");
        gate.release();
        let returned = syncing
            .await
            .map_err(|error| TestError::new(format!("the sync: {error:?}")))?
            .context("synced")?;
        assert!(synced.get());
        // Released, a sync passes the gate at once.
        returned.sync().await.context("synced again")?;
        Ok::<(), TestError>(())
    })??;
    Ok(())
}

/// `until_waiting` returns once a sync waits at the gate and not before, and once that sync has passed, waits for the next.
#[test]
fn test_until_waiting_returns_once_a_sync_waits_and_not_before() -> TestResult {
    run(async {
        let gate = SyncGate::default();
        let gated = GatedStorage::new(SimStorage::new(1), gate.clone());
        let file = Rc::new(
            gated
                .open(Path::new("/a"), OpenMode::Create)
                .await
                .context("opened")?,
        );
        for round in 0..2 {
            gate.hold();
            let seen = Rc::new(Cell::new(false));
            let watching = {
                let (gate, seen) = (gate.clone(), seen.clone());
                spawn(async move {
                    let outcome = gate.until_waiting().await;
                    seen.set(true);
                    outcome
                })
            };
            compio::time::sleep(WATCHED).await;
            assert!(!seen.get(), "round {round}: returned with no sync waiting");
            let syncing = {
                let file = file.clone();
                spawn(async move { file.sync().await })
            };
            watching
                .await
                .map_err(|error| TestError::new(format!("the wait: {error:?}")))?
                .context("a sync waits")?;
            gate.release();
            syncing
                .await
                .map_err(|error| TestError::new(format!("the sync: {error:?}")))?
                .context("synced")?;
        }
        Ok::<(), TestError>(())
    })??;
    Ok(())
}
