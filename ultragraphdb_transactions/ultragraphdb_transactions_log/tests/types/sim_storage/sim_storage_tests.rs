/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The simulated storage's model: buffered and stable contents of files and entries of directories, what a file sync,
//! a directory sync and a power loss do to them, and the four fault modes, each drawn from a seed.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use ultragraphdb_transactions_log::{
    AlignedBuf, DurableFile, DurableStorage, FaultMode, OpenMode, SimFile, SimStorage,
    StorageOperation,
};

use crate::utils::helpers::block_on;

fn path(name: &str) -> PathBuf {
    Path::new("/d").join(name)
}

/// A storage with the directory `/d` durable.
fn storage(seed: u64) -> SimStorage {
    let storage = SimStorage::new(seed);
    block_on(storage.create_directory(Path::new("/d"))).expect("created");
    block_on(storage.sync_directory(Path::new("/"))).expect("synced");
    storage
}

fn create(storage: &SimStorage, name: &str) -> SimFile {
    block_on(storage.open(&path(name), OpenMode::Create)).expect("created")
}

fn write(file: &mut SimFile, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
    block_on(file.write_at(offset, AlignedBuf::from_slice(bytes)))
}

#[test]
fn test_an_unsynced_write_is_lost_to_a_power_loss() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    write(&mut file, 0, b"synced").expect("written");
    block_on(file.sync()).expect("synced");
    write(&mut file, 0, b"later!!!").expect("written");
    assert_eq!(storage.contents(&path("f")), Some(b"later!!!".to_vec()));
    assert_eq!(
        storage.stable_contents(&path("f")),
        Some(b"synced".to_vec())
    );
    storage.power_loss();
    assert_eq!(storage.contents(&path("f")), Some(b"synced".to_vec()));
}

#[test]
fn test_a_file_sync_does_not_make_its_entry_durable() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    write(&mut file, 0, b"data").expect("written");
    block_on(file.sync()).expect("synced");
    assert_eq!(storage.stable_contents(&path("f")), None);
    storage.power_loss();
    assert_eq!(storage.contents(&path("f")), None);
    let gone = block_on(storage.open(&path("f"), OpenMode::Read)).expect_err("lost");
    assert_eq!(gone.kind(), ErrorKind::NotFound);
}

#[test]
fn test_a_directory_sync_makes_its_entries_durable_not_the_contents() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    write(&mut file, 0, b"data").expect("written");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    assert_eq!(storage.stable_contents(&path("f")), Some(Vec::new()));
    storage.power_loss();
    assert_eq!(storage.contents(&path("f")), Some(Vec::new()));
}

#[test]
fn test_a_rename_and_a_removal_last_only_once_their_directory_is_synced() {
    let storage = storage(1);
    let mut a = create(&storage, "a");
    write(&mut a, 0, b"A").expect("written");
    block_on(a.sync()).expect("synced");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    block_on(storage.rename(&path("a"), &path("b"))).expect("renamed");
    storage.power_loss();
    assert_eq!(storage.contents(&path("a")), Some(b"A".to_vec()));
    assert_eq!(storage.contents(&path("b")), None);
    block_on(storage.rename(&path("a"), &path("b"))).expect("renamed");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    storage.power_loss();
    assert_eq!(storage.contents(&path("b")), Some(b"A".to_vec()));
    block_on(storage.remove_file(&path("b"))).expect("removed");
    storage.power_loss();
    assert_eq!(storage.contents(&path("b")), Some(b"A".to_vec()));
    block_on(storage.remove_file(&path("b"))).expect("removed");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    storage.power_loss();
    assert_eq!(storage.contents(&path("b")), None);
}

#[test]
fn test_a_directory_not_synced_into_its_parent_is_lost() {
    let storage = SimStorage::new(1);
    block_on(storage.create_directory(Path::new("/x"))).expect("created");
    storage.power_loss();
    let gone = block_on(storage.list(Path::new("/x"))).expect_err("lost");
    assert_eq!(gone.kind(), ErrorKind::NotFound);
}

#[test]
fn test_the_trace_records_each_operation_and_its_path() {
    let storage = SimStorage::new(1);
    block_on(storage.create_directory(Path::new("/d"))).expect("created");
    let mut file = create(&storage, "f");
    write(&mut file, 0, b"x").expect("written");
    block_on(file.sync()).expect("synced");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    let _ = block_on(file.read_at(0, 1)).expect("read");
    let _ = block_on(file.length()).expect("length");
    block_on(file.preallocate(4_096)).expect("preallocated");
    block_on(file.truncate(1)).expect("cut");
    block_on(storage.rename(&path("f"), &path("g"))).expect("renamed");
    let _ = block_on(storage.list(Path::new("/d"))).expect("listed");
    let _ = block_on(storage.open(&path("g"), OpenMode::Read)).expect("opened");
    block_on(storage.remove_file(&path("g"))).expect("removed");
    let f = path("f");
    let d = PathBuf::from("/d");
    assert_eq!(
        storage.trace(),
        vec![
            (StorageOperation::CreateDirectory, d.clone()),
            (StorageOperation::Create, f.clone()),
            (StorageOperation::Write, f.clone()),
            (StorageOperation::FileSync, f.clone()),
            (StorageOperation::DirectorySync, d.clone()),
            (StorageOperation::Read, f.clone()),
            (StorageOperation::Length, f.clone()),
            (StorageOperation::Preallocate, f.clone()),
            (StorageOperation::Truncate, f.clone()),
            (StorageOperation::Rename, f.clone()),
            (StorageOperation::List, d),
            (StorageOperation::Open, path("g")),
            (StorageOperation::Remove, path("g")),
        ]
    );
}

#[test]
fn test_a_fault_before_fails_with_no_effect() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    let next = storage.trace().len();
    storage.fail_at(next, FaultMode::Before);
    let error = write(&mut file, 0, b"data").expect_err("faulted");
    assert_eq!(error.kind(), ErrorKind::Other);
    assert_eq!(storage.contents(&path("f")), Some(Vec::new()));
    // The fault fires once: the operation is in the trace, and the next one succeeds.
    assert_eq!(storage.trace().len(), next + 1);
    write(&mut file, 0, b"data").expect("written");
}

#[test]
fn test_a_fault_after_takes_effect_then_fails() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    storage.fail_at(storage.trace().len(), FaultMode::After);
    write(&mut file, 0, b"data").expect_err("faulted");
    assert_eq!(storage.contents(&path("f")), Some(b"data".to_vec()));
    storage.fail_at(storage.trace().len(), FaultMode::After);
    block_on(file.sync()).expect_err("faulted");
    assert_eq!(storage.stable_contents(&path("f")), Some(b"data".to_vec()));
}

#[test]
fn test_a_fault_before_a_sync_leaves_the_contents_unstable() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    write(&mut file, 0, b"data").expect("written");
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    block_on(file.sync()).expect_err("faulted");
    assert_eq!(storage.stable_contents(&path("f")), Some(Vec::new()));
}

/// Operation `index` of the twelve besides a write, each of which a fault reaches.
fn operation(index: usize, s: &SimStorage, f: &mut SimFile) -> std::io::Result<()> {
    match index {
        0 => block_on(s.open(&path("f"), OpenMode::Read)).map(|_| ()),
        1 => block_on(s.open(&path("n"), OpenMode::Create)).map(|_| ()),
        2 => block_on(f.read_at(0, 0)).map(|_| ()),
        3 => block_on(f.length()).map(|_| ()),
        4 => block_on(f.preallocate(10)),
        5 => block_on(f.truncate(10)),
        6 => block_on(f.sync()),
        7 => block_on(s.create_directory(Path::new("/e"))),
        8 => block_on(s.sync_directory(Path::new("/d"))),
        9 => block_on(s.rename(&path("n"), &path("m"))),
        10 => block_on(s.remove_file(&path("m"))),
        _ => block_on(s.list(Path::new("/d"))).map(|_| ()),
    }
}

#[test]
fn test_faults_reach_every_kind_of_operation() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    for index in 0..12 {
        storage.fail_at(storage.trace().len(), FaultMode::Before);
        let error = operation(index, &storage, &mut file).expect_err("faulted");
        assert_eq!(error.kind(), ErrorKind::Other, "operation {index}");
        operation(index, &storage, &mut file).expect("the next try succeeds");
    }
}

#[test]
fn test_a_torn_write_keeps_a_prefix_and_reports_it() {
    let storage = storage(7);
    let mut file = create(&storage, "f");
    let bytes = vec![0xAAu8; 16_384];
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    let error = write(&mut file, 0, &bytes).expect_err("torn");
    assert_eq!(error.kind(), ErrorKind::Other);
    let kept = storage.contents(&path("f")).expect("the file");
    // A tear keeps whole 512-byte sectors, at least one, and never the whole write.
    assert!(!kept.is_empty() && kept.len() < bytes.len());
    assert_eq!(kept.len() % 512, 0);
    assert!(kept.iter().all(|b| *b == 0xAA));
}

#[test]
fn test_a_silent_torn_write_keeps_a_prefix_and_reports_success() {
    let storage = storage(7);
    let mut file = create(&storage, "f");
    let bytes = vec![0xAAu8; 16_384];
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &bytes).expect("reported as written");
    let kept = storage.contents(&path("f")).expect("the file");
    assert!(!kept.is_empty() && kept.len() < bytes.len());
    assert_eq!(kept.len() % 512, 0);
}

#[test]
fn test_a_torn_write_over_existing_bytes_keeps_the_rest() {
    let storage = storage(3);
    let mut file = create(&storage, "f");
    write(&mut file, 0, &[0x11u8; 8_192]).expect("written");
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0x22u8; 8_192]).expect("reported as written");
    let kept = storage.contents(&path("f")).expect("the file");
    assert_eq!(kept.len(), 8_192);
    let cut = kept.iter().position(|b| *b == 0x11).expect("a tear");
    assert!(cut > 0 && cut % 512 == 0);
    assert!(kept[..cut].iter().all(|b| *b == 0x22));
    assert!(kept[cut..].iter().all(|b| *b == 0x11));
}

#[test]
fn test_a_write_shorter_than_a_sector_tears_inside_it() {
    let storage = storage(5);
    let mut file = create(&storage, "f");
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0x33u8; 64]).expect("reported as written");
    let kept = storage.contents(&path("f")).expect("the file");
    assert!(!kept.is_empty() && kept.len() < 64);
}

#[test]
fn test_torn_modes_on_other_operations() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    block_on(storage.sync_directory(Path::new("/d"))).expect("synced");
    write(&mut file, 0, b"data").expect("written");
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    block_on(file.sync()).expect_err("a torn mode fails another operation after its effect");
    assert_eq!(storage.stable_contents(&path("f")), Some(b"data".to_vec()));
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    block_on(file.truncate(2)).expect("a silent tear does not fault another operation");
    assert_eq!(storage.contents(&path("f")), Some(b"da".to_vec()));
}

/// The kept length of a 16,384-byte write torn on a storage seeded with `seed`.
fn tear_length(seed: u64) -> usize {
    let storage = storage(seed);
    let mut file = create(&storage, "f");
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0xAAu8; 16_384]).expect("reported as written");
    storage.contents(&path("f")).expect("the file").len()
}

#[test]
fn test_a_seed_replays_its_tears() {
    for seed in 0..20 {
        assert_eq!(tear_length(seed), tear_length(seed));
    }
    let lengths: std::collections::BTreeSet<usize> = (0..20).map(tear_length).collect();
    assert!(
        lengths.len() > 1,
        "different seeds tear at different points"
    );
}

/// Which of 200 writes fail, and how, on a storage seeded with `seed` faulting one operation in four.
fn random_faults(seed: u64) -> Vec<Option<ErrorKind>> {
    let storage = storage(seed);
    let mut file = create(&storage, "f");
    storage.fail_randomly(4);
    (0..200)
        .map(|i| write(&mut file, i * 8, b"12345678").err().map(|e| e.kind()))
        .collect()
}

#[test]
fn test_random_faults_replay_from_their_seed() {
    assert_eq!(random_faults(11), random_faults(11));
    assert_ne!(random_faults(11), random_faults(12));
    let faults = random_faults(11).iter().filter(|f| f.is_some()).count();
    // One in four, with three of the four modes reporting: about 37 of 200; far from none and from all.
    assert!(faults > 10 && faults < 80, "{faults} faults");
}

#[test]
fn test_random_faults_stop_at_zero() {
    let storage = storage(11);
    let mut file = create(&storage, "f");
    storage.fail_randomly(1);
    write(&mut file, 0, b"x").expect_err("every operation faults at one in one");
    storage.fail_randomly(0);
    for i in 0..50 {
        write(&mut file, i, b"x").expect("no fault");
    }
}

#[test]
fn test_put_file_is_durable_with_its_parents() {
    let storage = SimStorage::new(1);
    storage
        .put_file(Path::new("/a/b/c"), b"seeded")
        .expect("put");
    assert!(storage.trace().is_empty());
    storage.power_loss();
    assert_eq!(
        storage.contents(Path::new("/a/b/c")),
        Some(b"seeded".to_vec())
    );
    storage
        .put_file(Path::new("/a/b/c"), b"again")
        .expect("replaced");
    assert_eq!(
        storage.contents(Path::new("/a/b/c")),
        Some(b"again".to_vec())
    );
}

#[test]
fn test_corrupt_flips_both_views() {
    let storage = SimStorage::new(1);
    storage
        .put_file(Path::new("/f"), &[0x0F, 0x0F])
        .expect("put");
    storage.corrupt(Path::new("/f"), 1, 0xFF).expect("flipped");
    assert_eq!(storage.contents(Path::new("/f")), Some(vec![0x0F, 0xF0]));
    assert_eq!(
        storage.stable_contents(Path::new("/f")),
        Some(vec![0x0F, 0xF0])
    );
    let past = storage
        .corrupt(Path::new("/f"), 2, 0xFF)
        .expect_err("past the end");
    assert_eq!(past.kind(), ErrorKind::InvalidInput);
    let missing = storage
        .corrupt(Path::new("/g"), 0, 0xFF)
        .expect_err("missing");
    assert_eq!(missing.kind(), ErrorKind::NotFound);
}

#[test]
fn test_paths_are_absolute_and_plain() {
    let storage = SimStorage::new(1);
    for bad in ["relative", "/a/../b"] {
        let error = block_on(storage.open(Path::new(bad), OpenMode::Create)).expect_err(bad);
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{bad}");
    }
    let file = block_on(storage.open(Path::new("/f"), OpenMode::Create)).expect("created");
    drop(file);
    let not_dir = block_on(storage.open(Path::new("/f/g"), OpenMode::Create)).expect_err("a file");
    assert_eq!(not_dir.kind(), ErrorKind::NotADirectory);
    let dir = block_on(storage.open(Path::new("/"), OpenMode::Read)).expect_err("a directory");
    assert_eq!(dir.kind(), ErrorKind::IsADirectory);
    let list = block_on(storage.list(Path::new("/f"))).expect_err("a file");
    assert_eq!(list.kind(), ErrorKind::NotADirectory);
    let sync = block_on(storage.sync_directory(Path::new("/f"))).expect_err("a file");
    assert_eq!(sync.kind(), ErrorKind::NotADirectory);
}

#[test]
fn test_injected_lists_every_fault_that_fired() {
    let storage = storage(1);
    let mut file = create(&storage, "f");
    assert!(storage.injected().is_empty());
    let first = storage.trace().len();
    storage.fail_at(first, FaultMode::SilentTornWrite);
    write(&mut file, 0, &[1u8; 4_096]).expect("silently torn");
    let second = storage.trace().len();
    storage.fail_at(second, FaultMode::After);
    block_on(file.sync()).expect_err("faulted");
    // A fault set for an index already past does not fire.
    storage.fail_at(first, FaultMode::Before);
    write(&mut file, 0, b"x").expect("no fault");
    assert_eq!(
        storage.injected(),
        vec![
            (first, StorageOperation::Write, FaultMode::SilentTornWrite),
            (second, StorageOperation::FileSync, FaultMode::After),
        ]
    );
}

#[test]
fn test_a_directory_is_not_a_file() {
    let storage = storage(1);
    let create =
        block_on(storage.open(Path::new("/d"), OpenMode::Create)).expect_err("a directory");
    assert_eq!(create.kind(), ErrorKind::IsADirectory);
    assert_eq!(storage.contents(Path::new("/d")), None);
    assert_eq!(storage.stable_contents(Path::new("/d")), None);
    let flip = storage
        .corrupt(Path::new("/d"), 0, 1)
        .expect_err("a directory");
    assert_eq!(flip.kind(), ErrorKind::IsADirectory);
    let remove = block_on(storage.remove_file(Path::new("/d"))).expect_err("a directory");
    assert_eq!(remove.kind(), ErrorKind::IsADirectory);
    create_and_rename_over_a_directory(&storage);
}

fn create_and_rename_over_a_directory(storage: &SimStorage) {
    let _ = create(storage, "f");
    block_on(storage.create_directory(&path("sub"))).expect("created");
    let over = block_on(storage.rename(&path("f"), &path("sub"))).expect_err("a directory");
    assert_eq!(over.kind(), ErrorKind::IsADirectory);
}

#[test]
fn test_a_torn_one_byte_write_keeps_nothing() {
    let storage = storage(5);
    let mut file = create(&storage, "f");
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0x33]).expect("reported as written");
    assert_eq!(storage.contents(&path("f")), Some(Vec::new()));
}
