/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The simulated storage's model: buffered and stable contents of files and entries of directories, what a file sync,
//! a directory sync and a power loss do to them, and the four fault modes, each drawn from a seed.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use ultragraphdb_transactions_log::{
    AlignedBuf, DurableFile, DurableStorage, FaultMode, OpenMode, SimFile, SimStorage,
    StorageOperation,
};

use crate::utils::helpers::block_on;

fn path(name: &str) -> PathBuf {
    Path::new("/d").join(name)
}

/// A storage with the directory `/d` durable.
fn storage(seed: u64) -> TestResult<SimStorage> {
    let storage = SimStorage::new(seed);
    block_on(storage.create_directory(Path::new("/d"))).context("created")?;
    block_on(storage.sync_directory(Path::new("/"))).context("synced")?;
    Ok(storage)
}

fn create(storage: &SimStorage, name: &str) -> TestResult<SimFile> {
    block_on(storage.open(&path(name), OpenMode::Create)).context("created")
}

fn write(file: &mut SimFile, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
    block_on(file.write_at(offset, AlignedBuf::from_slice(bytes)))
}

/// Asserts `result` is the simulated storage's injected fault, reported `when` ("before" or "after") the operation
/// took effect, or either when `when` is empty.
fn assert_injected<T: std::fmt::Debug>(result: std::io::Result<T>, when: &str) -> TestResult {
    match result {
        Err(error) => {
            assert_eq!(error.kind(), ErrorKind::Other, "{error}");
            let message = error.to_string();
            assert!(
                message.starts_with(&format!("injected fault {when}"))
                    && message.ends_with(" the operation"),
                "{message}"
            );
            Ok(())
        }
        Ok(value) => Err(TestError::new(format!("not faulted: {value:?}"))),
    }
}

#[test]
fn test_an_unsynced_write_is_lost_to_a_power_loss() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    write(&mut file, 0, b"synced").context("written")?;
    block_on(file.sync()).context("synced")?;
    write(&mut file, 0, b"later!!!").context("written")?;
    assert_eq!(storage.contents(&path("f")), Some(b"later!!!".to_vec()));
    assert_eq!(
        storage.stable_contents(&path("f")),
        Some(b"synced".to_vec())
    );
    storage.power_loss();
    assert_eq!(storage.contents(&path("f")), Some(b"synced".to_vec()));
    Ok(())
}

#[test]
fn test_a_file_sync_does_not_make_its_entry_durable() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    write(&mut file, 0, b"data").context("written")?;
    block_on(file.sync()).context("synced")?;
    assert_eq!(storage.stable_contents(&path("f")), None);
    storage.power_loss();
    assert_eq!(storage.contents(&path("f")), None);
    let gone = block_on(storage.open(&path("f"), OpenMode::Read))
        .err()
        .context("lost")?;
    assert_eq!(gone.kind(), ErrorKind::NotFound);
    Ok(())
}

#[test]
fn test_a_directory_sync_makes_its_entries_durable_not_the_contents() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    write(&mut file, 0, b"data").context("written")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    assert_eq!(storage.stable_contents(&path("f")), Some(Vec::new()));
    storage.power_loss();
    assert_eq!(storage.contents(&path("f")), Some(Vec::new()));
    Ok(())
}

#[test]
fn test_a_rename_and_a_removal_last_only_once_their_directory_is_synced() -> TestResult {
    let storage = storage(1)?;
    let mut a = create(&storage, "a")?;
    write(&mut a, 0, b"A").context("written")?;
    block_on(a.sync()).context("synced")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    block_on(storage.rename(&path("a"), &path("b"))).context("renamed")?;
    storage.power_loss();
    assert_eq!(storage.contents(&path("a")), Some(b"A".to_vec()));
    assert_eq!(storage.contents(&path("b")), None);
    block_on(storage.rename(&path("a"), &path("b"))).context("renamed")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    storage.power_loss();
    assert_eq!(storage.contents(&path("b")), Some(b"A".to_vec()));
    block_on(storage.remove_file(&path("b"))).context("removed")?;
    storage.power_loss();
    assert_eq!(storage.contents(&path("b")), Some(b"A".to_vec()));
    block_on(storage.remove_file(&path("b"))).context("removed")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    storage.power_loss();
    assert_eq!(storage.contents(&path("b")), None);
    Ok(())
}

#[test]
fn test_a_directory_not_synced_into_its_parent_is_lost() -> TestResult {
    let storage = SimStorage::new(1);
    block_on(storage.create_directory(Path::new("/x"))).context("created")?;
    storage.power_loss();
    let gone = block_on(storage.list(Path::new("/x")))
        .err()
        .context("lost")?;
    assert_eq!(gone.kind(), ErrorKind::NotFound);
    Ok(())
}

#[test]
fn test_the_trace_records_each_operation_and_its_path() -> TestResult {
    let storage = SimStorage::new(1);
    block_on(storage.create_directory(Path::new("/d"))).context("created")?;
    let mut file = create(&storage, "f")?;
    write(&mut file, 0, b"x").context("written")?;
    block_on(file.sync()).context("synced")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    let _read: AlignedBuf = block_on(file.read_at(0, 1)).context("read")?;
    let _length: u64 = block_on(file.length()).context("length")?;
    block_on(file.preallocate(4_096)).context("preallocated")?;
    block_on(file.truncate(1)).context("cut")?;
    block_on(storage.rename(&path("f"), &path("g"))).context("renamed")?;
    let _listed: Vec<String> = block_on(storage.list(Path::new("/d"))).context("listed")?;
    let _opened: SimFile = block_on(storage.open(&path("g"), OpenMode::Read)).context("opened")?;
    block_on(storage.remove_file(&path("g"))).context("removed")?;
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
    Ok(())
}

#[test]
fn test_a_fault_before_fails_with_no_effect() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    let next = storage.trace().len();
    storage.fail_at(next, FaultMode::Before);
    let error = write(&mut file, 0, b"data").err().context("faulted")?;
    assert_eq!(error.kind(), ErrorKind::Other);
    assert_eq!(storage.contents(&path("f")), Some(Vec::new()));
    // The fault fires once: the operation is in the trace, and the next one succeeds.
    assert_eq!(storage.trace().len(), next + 1);
    write(&mut file, 0, b"data").context("written")?;
    Ok(())
}

#[test]
fn test_a_fault_after_takes_effect_then_fails() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    storage.fail_at(storage.trace().len(), FaultMode::After);
    assert_injected(write(&mut file, 0, b"data"), "after")?;
    assert_eq!(storage.contents(&path("f")), Some(b"data".to_vec()));
    storage.fail_at(storage.trace().len(), FaultMode::After);
    assert_injected(block_on(file.sync()), "after")?;
    assert_eq!(storage.stable_contents(&path("f")), Some(b"data".to_vec()));
    Ok(())
}

#[test]
fn test_a_fault_before_a_sync_leaves_the_contents_unstable() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    write(&mut file, 0, b"data").context("written")?;
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    assert_injected(block_on(file.sync()), "before")?;
    assert_eq!(storage.stable_contents(&path("f")), Some(Vec::new()));
    Ok(())
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
fn test_faults_reach_every_kind_of_operation() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    for index in 0..12 {
        storage.fail_at(storage.trace().len(), FaultMode::Before);
        let error = operation(index, &storage, &mut file)
            .err()
            .context("faulted")?;
        assert_eq!(error.kind(), ErrorKind::Other, "operation {index}");
        operation(index, &storage, &mut file).context("the next try succeeds")?;
    }
    Ok(())
}

#[test]
fn test_a_torn_write_keeps_a_prefix_and_reports_it() -> TestResult {
    let storage = storage(7)?;
    let mut file = create(&storage, "f")?;
    let bytes = vec![0xAAu8; 16_384];
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    let error = write(&mut file, 0, &bytes).err().context("torn")?;
    assert_eq!(error.kind(), ErrorKind::Other);
    let contents = storage.contents(&path("f")).context("the file")?;
    // The file system records the whole write, the drive kept a prefix: the file is as long as the write, and a tear
    // keeps whole 512-byte sectors of it, at least one and never all, the rest zeros.
    assert_eq!(contents.len(), bytes.len());
    let kept = prefix(&contents, 0xAA);
    assert!(kept > 0 && kept < bytes.len());
    assert_eq!(kept % 512, 0);
    assert!(contents[kept..].iter().all(|b| *b == 0));
    Ok(())
}

/// How many leading bytes of `bytes` are `value`.
fn prefix(bytes: &[u8], value: u8) -> usize {
    bytes.iter().take_while(|b| **b == value).count()
}

#[test]
fn test_a_silent_torn_write_keeps_a_prefix_and_reports_success() -> TestResult {
    let storage = storage(7)?;
    let mut file = create(&storage, "f")?;
    let bytes = vec![0xAAu8; 16_384];
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &bytes).context("reported as written")?;
    let contents = storage.contents(&path("f")).context("the file")?;
    assert_eq!(contents.len(), bytes.len());
    let kept = prefix(&contents, 0xAA);
    assert!(kept > 0 && kept < bytes.len());
    assert_eq!(kept % 512, 0);
    assert!(contents[kept..].iter().all(|b| *b == 0));
    Ok(())
}

#[test]
fn test_a_torn_write_over_existing_bytes_keeps_the_rest() -> TestResult {
    let storage = storage(3)?;
    let mut file = create(&storage, "f")?;
    write(&mut file, 0, &[0x11u8; 8_192]).context("written")?;
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0x22u8; 8_192]).context("reported as written")?;
    let kept = storage.contents(&path("f")).context("the file")?;
    assert_eq!(kept.len(), 8_192);
    let cut = kept.iter().position(|b| *b == 0x11).context("a tear")?;
    assert!(cut > 0 && cut % 512 == 0);
    assert!(kept[..cut].iter().all(|b| *b == 0x22));
    assert!(kept[cut..].iter().all(|b| *b == 0x11));
    Ok(())
}

#[test]
fn test_a_torn_write_past_the_end_keeps_the_old_bytes_and_zeros_after_them() -> TestResult {
    let storage = storage(3)?;
    let mut file = create(&storage, "f")?;
    write(&mut file, 0, &[0x11u8; 4_096]).context("written")?;
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0x22u8; 12_288]).context("reported as written")?;
    let contents = storage.contents(&path("f")).context("the file")?;
    assert_eq!(contents.len(), 12_288, "as long as the write");
    let kept = prefix(&contents, 0x22);
    assert!(kept > 0 && kept < 12_288 && kept.is_multiple_of(512));
    let old_end = kept.max(4_096);
    assert!(contents[kept..old_end].iter().all(|b| *b == 0x11));
    assert!(contents[old_end..].iter().all(|b| *b == 0));
    Ok(())
}

#[test]
fn test_a_short_torn_write_grows_the_file_only_as_far_as_its_prefix() -> TestResult {
    let storage = storage(7)?;
    let mut file = create(&storage, "f")?;
    write(&mut file, 0, &[0x11u8; 4_096]).context("written")?;
    storage.fail_at(storage.trace().len(), FaultMode::ShortTornWrite);
    let error = write(&mut file, 0, &[0xAAu8; 16_384])
        .err()
        .context("torn")?;
    assert_eq!(error.kind(), ErrorKind::Other);
    let contents = storage.contents(&path("f")).context("the file")?;
    let kept = prefix(&contents, 0xAA);
    assert!(kept > 0 && kept < 16_384 && kept.is_multiple_of(512));
    // The length is the prefix's end, or the old length where that lies further.
    assert_eq!(contents.len(), kept.max(4_096));
    assert!(contents[kept..].iter().all(|b| *b == 0x11));
    Ok(())
}

#[test]
fn test_a_random_tear_reported_with_an_error_is_whole_or_short() -> TestResult {
    let storage = storage(11)?;
    let mut file = create(&storage, "f")?;
    storage.fail_randomly(2);
    for i in 0..400 {
        // Writes fail at random by design; the faults injected are checked below.
        let _written: std::io::Result<()> = write(&mut file, i * 8, b"12345678");
    }
    let injected = storage.injected();
    for mode in [FaultMode::TornWrite, FaultMode::ShortTornWrite] {
        assert!(
            injected.iter().any(|(_, _, m)| *m == mode),
            "{mode:?} drawn"
        );
    }
    Ok(())
}

#[test]
fn test_a_refused_preallocation_is_unsupported() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    storage.refuse_preallocation();
    let before = storage.trace().len();
    let refused = block_on(file.preallocate(4_096)).err().context("refused")?;
    assert_eq!(refused.kind(), ErrorKind::Unsupported);
    assert_eq!(
        storage.trace()[before..],
        [(StorageOperation::Preallocate, path("f"))]
    );
    assert_eq!(storage.contents(&path("f")), Some(Vec::new()));
    Ok(())
}

#[test]
fn test_a_write_shorter_than_a_sector_tears_inside_it() -> TestResult {
    let storage = storage(5)?;
    let mut file = create(&storage, "f")?;
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0x33u8; 64]).context("reported as written")?;
    let contents = storage.contents(&path("f")).context("the file")?;
    assert_eq!(contents.len(), 64);
    let kept = prefix(&contents, 0x33);
    assert!(kept > 0 && kept < 64);
    assert!(contents[kept..].iter().all(|b| *b == 0));
    Ok(())
}

#[test]
fn test_torn_modes_on_other_operations() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    block_on(storage.sync_directory(Path::new("/d"))).context("synced")?;
    write(&mut file, 0, b"data").context("written")?;
    storage.fail_at(storage.trace().len(), FaultMode::TornWrite);
    // A torn mode fails another operation after its effect.
    assert_injected(block_on(file.sync()), "after")?;
    assert_eq!(storage.stable_contents(&path("f")), Some(b"data".to_vec()));
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    block_on(file.truncate(2)).context("a silent tear does not fault another operation")?;
    assert_eq!(storage.contents(&path("f")), Some(b"da".to_vec()));
    storage.fail_at(storage.trace().len(), FaultMode::ShortTornWrite);
    // A short tear fails another operation after its effect.
    assert_injected(block_on(file.truncate(1)), "after")?;
    assert_eq!(storage.contents(&path("f")), Some(b"d".to_vec()));
    Ok(())
}

/// The kept length of a 16,384-byte write torn on a storage seeded with `seed`.
fn tear_length(seed: u64) -> TestResult<usize> {
    let storage = storage(seed)?;
    let mut file = create(&storage, "f")?;
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0xAAu8; 16_384]).context("reported as written")?;
    Ok(prefix(
        &storage.contents(&path("f")).context("the file")?,
        0xAA,
    ))
}

#[test]
fn test_a_seed_replays_its_tears() -> TestResult {
    for seed in 0..20 {
        assert_eq!(tear_length(seed)?, tear_length(seed)?);
    }
    let lengths: std::collections::BTreeSet<usize> =
        (0..20).map(tear_length).collect::<TestResult<_>>()?;
    assert!(
        lengths.len() > 1,
        "different seeds tear at different points"
    );
    Ok(())
}

/// Which of 200 writes fail, and how, on a storage seeded with `seed` faulting one operation in four.
fn random_faults(seed: u64) -> TestResult<Vec<Option<ErrorKind>>> {
    let storage = storage(seed)?;
    let mut file = create(&storage, "f")?;
    storage.fail_randomly(4);
    Ok((0..200)
        .map(|i| write(&mut file, i * 8, b"12345678").err().map(|e| e.kind()))
        .collect())
}

#[test]
fn test_random_faults_replay_from_their_seed() -> TestResult {
    assert_eq!(random_faults(11)?, random_faults(11)?);
    assert_ne!(random_faults(11)?, random_faults(12)?);
    let faults = random_faults(11)?.iter().filter(|f| f.is_some()).count();
    // One in four, with three of the four modes reporting: about 37 of 200; far from none and from all.
    assert!(faults > 10 && faults < 80, "{faults} faults");
    Ok(())
}

#[test]
fn test_random_faults_stop_at_zero() -> TestResult {
    let storage = storage(11)?;
    let mut file = create(&storage, "f")?;
    storage.fail_randomly(1);
    // Every operation faults at one in one, in a mode drawn from the seed.
    assert_injected(write(&mut file, 0, b"x"), "")?;
    storage.fail_randomly(0);
    for i in 0..50 {
        write(&mut file, i, b"x").context("no fault")?;
    }
    Ok(())
}

#[test]
fn test_put_file_is_durable_with_its_parents() -> TestResult {
    let storage = SimStorage::new(1);
    storage
        .put_file(Path::new("/a/b/c"), b"seeded")
        .context("put")?;
    assert!(storage.trace().is_empty());
    storage.power_loss();
    assert_eq!(
        storage.contents(Path::new("/a/b/c")),
        Some(b"seeded".to_vec())
    );
    storage
        .put_file(Path::new("/a/b/c"), b"again")
        .context("replaced")?;
    assert_eq!(
        storage.contents(Path::new("/a/b/c")),
        Some(b"again".to_vec())
    );
    Ok(())
}

#[test]
fn test_corrupt_flips_both_views() -> TestResult {
    let storage = SimStorage::new(1);
    storage
        .put_file(Path::new("/f"), &[0x0F, 0x0F])
        .context("put")?;
    storage
        .corrupt(Path::new("/f"), 1, 0xFF)
        .context("flipped")?;
    assert_eq!(storage.contents(Path::new("/f")), Some(vec![0x0F, 0xF0]));
    assert_eq!(
        storage.stable_contents(Path::new("/f")),
        Some(vec![0x0F, 0xF0])
    );
    let past = storage
        .corrupt(Path::new("/f"), 2, 0xFF)
        .err()
        .context("past the end")?;
    assert_eq!(past.kind(), ErrorKind::InvalidInput);
    let missing = storage
        .corrupt(Path::new("/g"), 0, 0xFF)
        .err()
        .context("missing")?;
    assert_eq!(missing.kind(), ErrorKind::NotFound);
    Ok(())
}

#[test]
fn test_paths_are_absolute_and_plain() -> TestResult {
    let storage = SimStorage::new(1);
    for bad in ["relative", "/a/../b"] {
        let error = block_on(storage.open(Path::new(bad), OpenMode::Create))
            .err()
            .context(bad)?;
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{bad}");
    }
    let file = block_on(storage.open(Path::new("/f"), OpenMode::Create)).context("created")?;
    drop(file);
    let not_dir = block_on(storage.open(Path::new("/f/g"), OpenMode::Create))
        .err()
        .context("a file")?;
    assert_eq!(not_dir.kind(), ErrorKind::NotADirectory);
    let dir = block_on(storage.open(Path::new("/"), OpenMode::Read))
        .err()
        .context("a directory")?;
    assert_eq!(dir.kind(), ErrorKind::IsADirectory);
    let list = block_on(storage.list(Path::new("/f")))
        .err()
        .context("a file")?;
    assert_eq!(list.kind(), ErrorKind::NotADirectory);
    let sync = block_on(storage.sync_directory(Path::new("/f")))
        .err()
        .context("a file")?;
    assert_eq!(sync.kind(), ErrorKind::NotADirectory);
    Ok(())
}

#[test]
fn test_injected_lists_every_fault_that_fired() -> TestResult {
    let storage = storage(1)?;
    let mut file = create(&storage, "f")?;
    assert!(storage.injected().is_empty());
    let first = storage.trace().len();
    storage.fail_at(first, FaultMode::SilentTornWrite);
    write(&mut file, 0, &[1u8; 4_096]).context("silently torn")?;
    let second = storage.trace().len();
    storage.fail_at(second, FaultMode::After);
    assert_injected(block_on(file.sync()), "after")?;
    // A fault set for an index already past does not fire.
    storage.fail_at(first, FaultMode::Before);
    write(&mut file, 0, b"x").context("no fault")?;
    assert_eq!(
        storage.injected(),
        vec![
            (first, StorageOperation::Write, FaultMode::SilentTornWrite),
            (second, StorageOperation::FileSync, FaultMode::After),
        ]
    );
    Ok(())
}

#[test]
fn test_a_directory_is_not_a_file() -> TestResult {
    let storage = storage(1)?;
    let create = block_on(storage.open(Path::new("/d"), OpenMode::Create))
        .err()
        .context("a directory")?;
    assert_eq!(create.kind(), ErrorKind::IsADirectory);
    assert_eq!(storage.contents(Path::new("/d")), None);
    assert_eq!(storage.stable_contents(Path::new("/d")), None);
    let flip = storage
        .corrupt(Path::new("/d"), 0, 1)
        .err()
        .context("a directory")?;
    assert_eq!(flip.kind(), ErrorKind::IsADirectory);
    let remove = block_on(storage.remove_file(Path::new("/d")))
        .err()
        .context("a directory")?;
    assert_eq!(remove.kind(), ErrorKind::IsADirectory);
    create_and_rename_over_a_directory(&storage)?;
    Ok(())
}

fn create_and_rename_over_a_directory(storage: &SimStorage) -> TestResult {
    let _created: SimFile = create(storage, "f")?;
    block_on(storage.create_directory(&path("sub"))).context("created")?;
    let over = block_on(storage.rename(&path("f"), &path("sub")))
        .err()
        .context("a directory")?;
    assert_eq!(over.kind(), ErrorKind::IsADirectory);
    Ok(())
}

#[test]
fn test_a_torn_one_byte_write_keeps_nothing() -> TestResult {
    let storage = storage(5)?;
    let mut file = create(&storage, "f")?;
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0x33]).context("reported as written")?;
    assert_eq!(storage.contents(&path("f")), Some(vec![0]));
    Ok(())
}

/// The modes the storage draws are SplitMix64's (Steele, Lea and Flood, OOPSLA 2014) from its seed. Faulting every
/// operation, each operation takes a draw for the one-in-one test, a draw modulo 4 for the mode (Before, After, a
/// reported tear, a silent tear) and, for a reported tear, a draw modulo 2 (whole length or short). The expected modes
/// for seed 11 are computed by an independent Python SplitMix64 (checked against the generator's published first
/// outputs for seed 0, 0xE220A8397B1DCDAF and 0x6E789E6AA1B965F4); a listing tears nothing, so no other draw intervenes.
/// A silent tear faults only a write, so on a listing the sixth draw's silent tear is drawn and not injected.
#[test]
fn test_random_fault_modes_are_splitmix64_draws_from_the_seed() -> TestResult {
    use FaultMode::{After, Before, ShortTornWrite, TornWrite};
    let storage = storage(11)?;
    storage.fail_randomly(1);
    for _ in 0..12 {
        // Every listing is faulted; only the modes drawn are under test.
        let _listed: std::io::Result<Vec<String>> = block_on(storage.list(Path::new("/d")));
    }
    let modes: Vec<FaultMode> = storage.injected().iter().map(|(_, _, m)| *m).collect();
    assert_eq!(
        modes,
        [
            After,
            Before,
            TornWrite,
            TornWrite,
            After,
            Before,
            Before,
            TornWrite,
            ShortTornWrite,
            TornWrite,
            After
        ]
    );
    Ok(())
}

/// The kept length of a tear is the seed's first SplitMix64 draw, as the independent Python SplitMix64 above computes
/// it: for a write past one sector, `512 * (1 + draw % ((len - 1) / 512))` whole sectors, for one of at most a sector
/// `1 + draw % (len - 1)` bytes. A 16,384-byte write: seed 0 draws 8,704 bytes; seed 10, whose draw is 0 modulo 31,
/// one sector; seed 13, whose draw is 31 modulo 32, six sectors. A 512-byte write, one sector, keeps 416 bytes at
/// seed 0, never the whole sector.
#[test]
fn test_a_tear_keeps_the_seeds_draw_of_whole_sectors_and_never_all() -> TestResult {
    assert_eq!(tear_length(0)?, 8_704);
    assert_eq!(tear_length(10)?, 512);
    assert_eq!(tear_length(13)?, 3_072);
    let storage = storage(0)?;
    let mut file = create(&storage, "f")?;
    storage.fail_at(storage.trace().len(), FaultMode::SilentTornWrite);
    write(&mut file, 0, &[0xAAu8; 512]).context("reported as written")?;
    let contents = storage.contents(&path("f")).context("the file")?;
    assert_eq!(prefix(&contents, 0xAA), 416);
    Ok(())
}
