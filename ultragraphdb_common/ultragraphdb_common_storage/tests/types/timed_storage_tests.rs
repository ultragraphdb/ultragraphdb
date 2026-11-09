/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::Path;
use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_storage::{LogIo, TimedStorage};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_log::{
    AlignedBuf, DurableFile, DurableStorage, FaultMode, OpenMode, SimStorage,
};

/// The storage under test over the simulated storage, which the test keeps to look into, timed on the runtime's clock.
async fn timed(sim: &SimStorage) -> TimedStorage<SimStorage> {
    let origin = compio::time::interval(Duration::from_secs(1)).tick().await;
    TimedStorage::new(sim.clone(), LogIo::default(), origin)
}

/// A file's writes count with their bytes and its syncs and a directory's count as syncs, and what they wrote is the inner
/// storage's.
#[test]
fn test_writes_and_syncs_are_counted() -> TestResult {
    Runtime::new()?.block_on(async {
        let sim = SimStorage::new(1);
        let storage = timed(&sim).await;
        let dir = Path::new("/log");
        storage.create_directory(dir).await?;
        let path = dir.join("segment");
        let mut file = storage.open_aligned(&path, OpenMode::Create).await?;
        file.write_at(0, AlignedBuf::from_slice(&[7; 4_096]))
            .await?;
        file.write_at(4_096, AlignedBuf::from_slice(&[8; 8_192]))
            .await?;
        file.sync().await?;
        storage.sync_directory(dir).await?;
        let totals = storage.io().totals();
        assert_eq!(totals.writes(), 2);
        assert_eq!(totals.written_bytes(), 12_288);
        assert_eq!(totals.syncs(), 2);
        assert_eq!(file.length().await?, 12_288);
        let held = sim.contents(&path).context("the segment")?;
        assert_eq!(held.len(), 12_288);
        assert_eq!(held.first(), Some(&7));
        Ok(())
    })
}

/// A failed write and a failed sync count as done, and fail as the inner storage's did.
#[test]
fn test_failed_io_is_counted_and_returned() -> TestResult {
    Runtime::new()?.block_on(async {
        let sim = SimStorage::new(1);
        let storage = timed(&sim).await;
        let path = Path::new("/segment");
        let mut file = storage.open_aligned(path, OpenMode::Create).await?;
        let next = sim.trace().len();
        sim.fail_at(next, FaultMode::Before);
        sim.fail_at(next + 1, FaultMode::Before);
        assert!(
            file.write_at(0, AlignedBuf::from_slice(&[1; 4_096]))
                .await
                .is_err()
        );
        assert!(file.sync().await.is_err());
        let totals = storage.io().totals();
        assert_eq!((totals.writes(), totals.syncs()), (1, 1));
        Ok(())
    })
}

/// What it does not time is the inner storage's alone: the listing, renames, removals, reads and truncation.
#[test]
fn test_the_rest_is_the_inner_storages() -> TestResult {
    Runtime::new()?.block_on(async {
        let sim = SimStorage::new(1);
        let storage = timed(&sim).await;
        let dir = Path::new("/log");
        storage.create_directory(dir).await?;
        let (first, second) = (dir.join("a"), dir.join("b"));
        let mut file = storage.open(&first, OpenMode::Create).await?;
        file.write_at(0, AlignedBuf::from_slice(b"held")).await?;
        file.truncate(2).await?;
        assert_eq!(file.read_at(0, 2).await?.as_slice(), b"he");
        file.preallocate(4_096).await?;
        storage.rename(&first, &second).await?;
        assert_eq!(storage.list(dir).await?, vec!["b".to_string()]);
        storage.remove_file(&second).await?;
        assert!(storage.list(dir).await?.is_empty());
        assert_eq!(storage.io().totals().syncs(), 0);
        Ok(())
    })
}
