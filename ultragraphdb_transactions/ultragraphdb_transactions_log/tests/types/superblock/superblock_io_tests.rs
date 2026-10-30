/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::ErrorKind;
use std::path::Path;

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
        0,
        sequence,
    )
}

fn storage() -> SimStorage {
    let storage = SimStorage::new(1);
    block_on(storage.create_directory(&dir())).expect("created");
    block_on(storage.sync_directory(Path::new("/"))).expect("synced");
    storage
}

fn read(storage: &SimStorage) -> SuperblockRead {
    block_on(Superblock::read(storage, &dir())).expect("read")
}

#[test]
fn test_no_copy_is_empty() {
    assert_eq!(read(&storage()), SuperblockRead::Empty);
}

#[test]
fn test_empty_files_are_absent_copies() {
    let storage = storage();
    storage.put_file(&superblock('a'), &[]).expect("put");
    storage.put_file(&superblock('b'), &[]).expect("put");
    assert_eq!(read(&storage), SuperblockRead::Empty);
}

#[test]
fn test_one_copy_beside_an_absent_one_is_present() {
    let storage = storage();
    storage
        .put_file(&superblock('b'), &record(2).encode())
        .expect("put");
    assert_eq!(read(&storage), SuperblockRead::Present(record(2)));
    let storage = self::storage();
    storage
        .put_file(&superblock('a'), &record(3).encode())
        .expect("put");
    storage.put_file(&superblock('b'), &[]).expect("put");
    assert_eq!(read(&storage), SuperblockRead::Present(record(3)));
}

#[test]
fn test_the_higher_sequence_wins_in_either_copy() {
    let storage = storage();
    storage
        .put_file(&superblock('a'), &record(3).encode())
        .expect("put");
    storage
        .put_file(&superblock('b'), &record(4).encode())
        .expect("put");
    assert_eq!(read(&storage), SuperblockRead::Present(record(4)));
    storage
        .put_file(&superblock('a'), &record(5).encode())
        .expect("put");
    assert_eq!(read(&storage), SuperblockRead::Present(record(5)));
}

#[test]
fn test_a_copy_that_does_not_verify_is_unreadable_whatever_the_other_holds() {
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
        let storage = storage();
        if let Some(bytes) = a {
            storage.put_file(&superblock('a'), &bytes).expect("put");
        }
        if let Some(bytes) = b {
            storage.put_file(&superblock('b'), &bytes).expect("put");
        }
        assert_eq!(read(&storage), SuperblockRead::Unreadable { copy });
    }
}

/// Asserts `error` is the storage's failure of `operation`, as the simulated storage reports an injected fault.
fn assert_io(error: &LogError, operation: &str) {
    match &error.0 {
        LogErrorEnum::Io {
            operation: named,
            kind,
            ..
        } => assert_eq!((*named, *kind), (operation, ErrorKind::Other), "{error:?}"),
        other => panic!("not an I/O error of {operation}: {other:?}"),
    }
}

#[test]
fn test_a_failed_read_is_an_io_error() {
    // Copy a's steps: its open, its length, its read; a failure at each names it.
    for (step, operation) in [(0, "open"), (1, "length"), (2, "read")] {
        let storage = storage();
        storage
            .put_file(&superblock('a'), &record(1).encode())
            .expect("put");
        storage.fail_at(storage.trace().len() + step, FaultMode::Before);
        let error = block_on(Superblock::read(&storage, &dir())).expect_err("faulted");
        assert_io(&error, operation);
    }
}

#[test]
fn test_a_write_goes_through_a_synced_temporary_renamed_over_its_copy() {
    let storage = storage();
    let before = storage.trace().len();
    block_on(record(1).write(&storage, &dir())).expect("written");
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
    block_on(record(2).write(&storage, &dir())).expect("written");
    assert_eq!(
        storage.contents(&superblock('b')),
        Some(record(2).encode().to_vec())
    );
    assert_eq!(read(&storage), SuperblockRead::Present(record(2)));
}

#[test]
fn test_a_write_failing_at_any_step_leaves_the_last_record() {
    // Every step of a write, each fault mode, then a power loss: the record read back is the last one written whole
    // or, when the failure came after its rename and directory sync took effect, the new one; never unreadable.
    for step in 0..5 {
        for mode in [
            FaultMode::Before,
            FaultMode::After,
            FaultMode::TornWrite,
            FaultMode::SilentTornWrite,
        ] {
            let storage = storage();
            block_on(record(1).write(&storage, &dir())).expect("written");
            storage.fail_at(storage.trace().len() + step, mode);
            let result = block_on(record(2).write(&storage, &dir()));
            storage.power_loss();
            let found = read(&storage);
            // A silent tear faults only a write and reports success; every other mode reports the step's failure.
            if mode == FaultMode::SilentTornWrite {
                assert_eq!(result, Ok(()), "step {step}");
            } else {
                assert_io(&result.expect_err("the step fails"), OPERATIONS[step]);
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
}

/// A write's five steps, as `write` names each in its error.
const OPERATIONS: [&str; 5] = ["create", "write", "file sync", "rename", "directory sync"];

#[test]
fn test_a_failed_write_is_an_io_error() {
    for (step, operation) in OPERATIONS.iter().enumerate() {
        let storage = storage();
        storage.fail_at(storage.trace().len() + step, FaultMode::Before);
        let error = block_on(record(1).write(&storage, &dir())).expect_err("faulted");
        assert_io(&error, operation);
    }
}

#[test]
fn test_the_copies_on_compio() {
    let tmp = TempDir::new("superblock");
    let (first, second) = run(async {
        let empty = Superblock::read(&CompioStorage, tmp.path())
            .await
            .expect("read");
        assert_eq!(empty, SuperblockRead::Empty);
        record(1)
            .write(&CompioStorage, tmp.path())
            .await
            .expect("written");
        record(2)
            .write(&CompioStorage, tmp.path())
            .await
            .expect("written");
        let names = CompioStorage.list(tmp.path()).await.expect("listed");
        assert_eq!(names, ["superblock.a", "superblock.b"]);
        let second = Superblock::read(&CompioStorage, tmp.path())
            .await
            .expect("read");
        (
            std::fs::read(tmp.path().join("superblock.a")).expect("a"),
            second,
        )
    });
    assert_eq!(first, record(1).encode().to_vec());
    assert_eq!(second, SuperblockRead::Present(record(2)));
}
