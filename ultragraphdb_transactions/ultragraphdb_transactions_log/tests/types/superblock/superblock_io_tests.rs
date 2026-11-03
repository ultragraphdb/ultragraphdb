/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::ErrorKind;
use std::path::Path;
use ultragraphdb_tool_bdd::{Context, TestResult};

use ultragraphdb_error_log::{LogError, LogErrorEnum};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::{
    CompioStorage, DurableStorage, FaultMode, SimStorage, StorageOperation, Superblock,
    SuperblockRead,
};

use crate::utils::helpers::{TempDir, block_on, dir, run, superblock};

fn record(sequence: u64) -> Superblock {
    Superblock::new(
        sequence,
        11,
        1,
        Position::new(sequence * 10),
        sequence * 100,
        sequence,
    )
}

fn storage() -> TestResult<SimStorage> {
    let storage = SimStorage::new(1);
    block_on(storage.create_directory(&dir())).context("created")?;
    block_on(storage.sync_directory(Path::new("/"))).context("synced")?;
    Ok(storage)
}

fn read(storage: &SimStorage) -> TestResult<SuperblockRead> {
    block_on(Superblock::read(storage, &dir())).context("read")
}

#[test]
fn test_no_copy_is_empty() -> TestResult {
    assert_eq!(read(&storage()?)?, SuperblockRead::Empty);
    Ok(())
}

#[test]
fn test_empty_files_are_absent_copies() -> TestResult {
    let storage = storage()?;
    storage.put_file(&superblock('a'), &[]).context("put")?;
    storage.put_file(&superblock('b'), &[]).context("put")?;
    assert_eq!(read(&storage)?, SuperblockRead::Empty);
    Ok(())
}

#[test]
fn test_one_copy_beside_an_absent_one_is_present() -> TestResult {
    let storage = storage()?;
    storage
        .put_file(&superblock('b'), &record(2).encode())
        .context("put")?;
    assert_eq!(read(&storage)?, SuperblockRead::Present(record(2)));
    let other = self::storage()?;
    other
        .put_file(&superblock('a'), &record(3).encode())
        .context("put")?;
    other.put_file(&superblock('b'), &[]).context("put")?;
    assert_eq!(read(&other)?, SuperblockRead::Present(record(3)));
    Ok(())
}

#[test]
fn test_the_higher_sequence_wins_in_either_copy() -> TestResult {
    let storage = storage()?;
    storage
        .put_file(&superblock('a'), &record(3).encode())
        .context("put")?;
    storage
        .put_file(&superblock('b'), &record(4).encode())
        .context("put")?;
    assert_eq!(read(&storage)?, SuperblockRead::Present(record(4)));
    storage
        .put_file(&superblock('a'), &record(5).encode())
        .context("put")?;
    assert_eq!(read(&storage)?, SuperblockRead::Present(record(5)));
    Ok(())
}

#[test]
fn test_a_copy_that_does_not_verify_is_unreadable_whatever_the_other_holds() -> TestResult {
    let mut torn = record(4).encode().to_vec();
    torn[20] ^= 0x01;
    let cases = [
        (Some(torn.clone()), Some(record(3).encode().to_vec()), 'a'),
        (Some(record(3).encode().to_vec()), Some(torn.clone()), 'b'),
        (Some(torn.clone()), None, 'a'),
        (None, Some(torn.clone()), 'b'),
        (Some(torn.clone()), Some(torn.clone()), 'a'),
        (Some(vec![1, 2, 3]), None, 'a'),
    ];
    for (a, b, copy) in cases {
        let storage = storage()?;
        if let Some(bytes) = a {
            storage.put_file(&superblock('a'), &bytes).context("put")?;
        }
        if let Some(bytes) = b {
            storage.put_file(&superblock('b'), &bytes).context("put")?;
        }
        assert_eq!(read(&storage)?, SuperblockRead::Unreadable { copy });
    }
    Ok(())
}

/// Asserts `error` is the storage's failure of `operation`, as the simulated storage reports an injected fault.
fn assert_io(error: &LogError, operation: &str) {
    assert!(
        matches!(
            &error.0,
            LogErrorEnum::Io { operation: named, kind: ErrorKind::Other, .. } if *named == operation
        ),
        "not an I/O error of {operation}: {error:?}"
    );
}

#[test]
fn test_a_failed_read_is_an_io_error() -> TestResult {
    // Copy a's steps: its open, its length, its read; a failure at each names it.
    for (step, operation) in [(0, "open"), (1, "length"), (2, "read")] {
        let storage = storage()?;
        storage
            .put_file(&superblock('a'), &record(1).encode())
            .context("put")?;
        storage.fail_at(storage.trace().len() + step, FaultMode::Before);
        let error = block_on(Superblock::read(&storage, &dir()))
            .err()
            .context("faulted")?;
        assert_io(&error, operation);
    }
    Ok(())
}

#[test]
fn test_a_write_goes_through_a_synced_temporary_renamed_over_its_copy() -> TestResult {
    let storage = storage()?;
    let before = storage.trace().len();
    block_on(record(1).write(&storage, &dir())).context("written")?;
    let temporary = dir().join("superblock.a.tmp");
    let steps: Vec<_> = storage.trace()[before..].to_vec();
    assert_eq!(
        steps,
        vec![
            (StorageOperation::Create, temporary.clone()),
            (StorageOperation::Write, temporary.clone()),
            (StorageOperation::FileSync, temporary.clone()),
            (StorageOperation::Rename, temporary),
            (StorageOperation::DirectorySync, dir()),
        ]
    );
    assert_eq!(
        storage.stable_contents(&superblock('a')),
        Some(record(1).encode().to_vec())
    );
    assert_eq!(storage.contents(&dir().join("superblock.a.tmp")), None);
    block_on(record(2).write(&storage, &dir())).context("written")?;
    assert_eq!(
        storage.contents(&superblock('b')),
        Some(record(2).encode().to_vec())
    );
    assert_eq!(read(&storage)?, SuperblockRead::Present(record(2)));
    Ok(())
}

#[test]
fn test_a_write_failing_at_any_step_leaves_the_last_record() -> TestResult {
    // Every step of a write, each fault mode, then a power loss: the record read back is the last one written whole
    // or, when the failure came after its rename and directory sync took effect, the new one; never unreadable.
    for (step, &operation) in OPERATIONS.iter().enumerate() {
        for mode in [
            FaultMode::Before,
            FaultMode::After,
            FaultMode::TornWrite,
            FaultMode::SilentTornWrite,
        ] {
            let storage = storage()?;
            block_on(record(1).write(&storage, &dir())).context("written")?;
            storage.fail_at(storage.trace().len() + step, mode);
            let result = block_on(record(2).write(&storage, &dir()));
            storage.power_loss();
            let found = read(&storage)?;
            // A silent tear faults only a write and reports success; every other mode reports the step's failure.
            if mode == FaultMode::SilentTornWrite {
                assert_eq!(result, Ok(()), "step {step}");
            } else {
                assert_io(&result.err().context("the step fails")?, operation);
            }
            // What survives the power loss: the new record only once its rename's directory sync took effect, which
            // is a fault after step 4 (Before skips it; a torn mode on a sync takes effect, as After does) or any
            // write that reported success; a silent tear of the temporary's write renames in a copy that does not
            // verify, refused, never fresh; anything else leaves the last record.
            let expected = match (mode, step) {
                (FaultMode::SilentTornWrite, 1) => SuperblockRead::Unreadable { copy: 'b' },
                (FaultMode::SilentTornWrite, _) => SuperblockRead::Present(record(2)),
                (FaultMode::After | FaultMode::TornWrite, 4) => SuperblockRead::Present(record(2)),
                _ => SuperblockRead::Present(record(1)),
            };
            assert_eq!(found, expected, "step {step} {mode:?}");
        }
    }
    Ok(())
}

/// A write's five steps, as `write` names each in its error.
const OPERATIONS: [&str; 5] = ["create", "write", "file sync", "rename", "directory sync"];

#[test]
fn test_a_failed_write_is_an_io_error() -> TestResult {
    for (step, operation) in OPERATIONS.iter().enumerate() {
        let storage = storage()?;
        storage.fail_at(storage.trace().len() + step, FaultMode::Before);
        let error = block_on(record(1).write(&storage, &dir()))
            .err()
            .context("faulted")?;
        assert_io(&error, operation);
    }
    Ok(())
}

#[test]
fn test_the_copies_on_compio() -> TestResult {
    let tmp = TempDir::new("superblock")?;
    let (first, second) = run(async {
        let empty = Superblock::read(&CompioStorage, tmp.path())
            .await
            .context("read")?;
        assert_eq!(empty, SuperblockRead::Empty);
        record(1)
            .write(&CompioStorage, tmp.path())
            .await
            .context("written")?;
        record(2)
            .write(&CompioStorage, tmp.path())
            .await
            .context("written")?;
        let names = CompioStorage.list(tmp.path()).await.context("listed")?;
        assert_eq!(names, ["superblock.a", "superblock.b"]);
        let second = Superblock::read(&CompioStorage, tmp.path())
            .await
            .context("read")?;
        TestResult::Ok((
            std::fs::read(tmp.path().join("superblock.a")).context("a")?,
            second,
        ))
    })??;
    assert_eq!(first, record(1).encode().to_vec());
    assert_eq!(second, SuperblockRead::Present(record(2)));
    Ok(())
}
