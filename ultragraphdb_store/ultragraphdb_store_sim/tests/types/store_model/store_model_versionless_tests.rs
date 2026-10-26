/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The shadow model's rules for the keyspace without versions, fed logs by hand: a read answers as the state it
//! reads, a batch's versionless writes survive with its entries or not at all, and only a drop of everything clears
//! them. Every expected detail is derived by hand from the log the test writes: `hex(b"k")` is `6b`, `b"1"` is
//! `[49]`, and so on.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_sim::{
    Contents, DropStage, FaultConfig, Invariant, OpRecord, Seed, SimClock, SimStore, StoreModel,
};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

/// A batch writing `entries`, then the versionless `writes` in order, a `None` value a removal.
fn batch(entries: Vec<Entry>, writes: &[(&[u8], Option<&[u8]>)]) -> WriteBatch {
    let mut batch = WriteBatch::new();
    entries.into_iter().for_each(|e| batch.push(e));
    for (key, value) in writes {
        match value {
            Some(value) => batch.put_versionless(key.to_vec(), value.to_vec()),
            None => batch.remove_versionless(key.to_vec()),
        }
    }
    batch
}

fn put(key: &[u8], version: u64) -> Entry {
    Entry::put(key.to_vec(), version, 0x04, vec![version as u8])
}

fn applied(batch: WriteBatch, mode: PersistMode) -> OpRecord {
    OpRecord::Applied { batch, mode }
}

fn read(id: Option<u64>, key: &[u8], result: Result<Option<Vec<u8>>, StoreError>) -> OpRecord {
    OpRecord::ReadVersionless {
        id,
        key: key.to_vec(),
        result,
    }
}

fn failed(key: &[u8]) -> StoreError {
    StoreError::ReadFailed(key.to_vec(), "x".into())
}

/// The contents of a fresh store after `batches` are applied, each buffered.
fn holding(batches: Vec<WriteBatch>) -> Contents {
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    batches
        .into_iter()
        .for_each(|b| store.apply(b, PersistMode::Buffer).unwrap());
    store.contents()
}

/// Asserts that `record` breaks the read invariant with `detail`, leaving `model` as it was.
fn breaks_read(model: &StoreModel, record: OpRecord, detail: &str) {
    let breach = model.clone().observe(&[record]).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (Invariant::ReadsAtOrBelowTimestamp, detail)
    );
}

/// A snapshot reads the versionless keys of the state it opened on, the store those of the latest batch, and a
/// temporary keyspace's snapshot none; a read the fault stream failed is accepted, any other failure or value is not.
#[test]
fn test_a_versionless_read_answers_as_the_state_it_reads() {
    let mut model = StoreModel::new();
    model
        .observe(&[
            applied(batch(vec![], &[(b"k", Some(b"1"))]), PersistMode::Buffer),
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
            applied(
                batch(vec![], &[(b"k", Some(b"2")), (b"j", Some(b"3"))]),
                PersistMode::Buffer,
            ),
            OpRecord::TemporaryCreated { temporary: 1 },
            OpRecord::TemporarySnapshotOpened {
                id: 1,
                temporary: 1,
            },
        ])
        .unwrap();
    model
        .observe(&[
            read(Some(0), b"k", Ok(Some(b"1".to_vec()))),
            read(Some(0), b"j", Ok(None)),
            read(None, b"k", Ok(Some(b"2".to_vec()))),
            read(None, b"j", Ok(Some(b"3".to_vec()))),
            read(None, b"z", Ok(None)),
            read(Some(1), b"k", Ok(None)),
            read(Some(0), b"k", Err(failed(b"k"))),
            read(None, b"k", Err(failed(b"k"))),
            read(Some(1), b"k", Err(failed(b"k"))),
            // Snapshot 9 was never opened: its reads are not checked.
            read(Some(9), b"k", Ok(Some(b"9".to_vec()))),
        ])
        .expect("each read answers as its state");
    let wrong = [
        (
            read(Some(0), b"k", Ok(Some(b"2".to_vec()))),
            "a versionless read of key 6b returned Ok(Some([50])) where Some([49]) was expected",
        ),
        (
            read(Some(0), b"j", Ok(Some(b"3".to_vec()))),
            "a versionless read of key 6a returned Ok(Some([51])) where None was expected",
        ),
        (
            read(None, b"k", Ok(Some(b"1".to_vec()))),
            "a versionless read of key 6b returned Ok(Some([49])) where Some([50]) was expected",
        ),
        (
            read(None, b"j", Ok(None)),
            "a versionless read of key 6a returned Ok(None) where Some([51]) was expected",
        ),
        (
            read(Some(1), b"k", Ok(Some(b"2".to_vec()))),
            "a versionless read of key 6b returned Ok(Some([50])) where None was expected",
        ),
        (
            read(None, b"k", Err(StoreError::Poisoned())),
            "a versionless read of key 6b returned Err(StoreError(Poisoned)) where Some([50]) was expected",
        ),
        (
            read(
                Some(0),
                b"k",
                Err(StoreError::Corrupted(b"k".to_vec(), "x".into())),
            ),
            "a versionless read of key 6b returned Err(StoreError(Corrupted { key: [107], reason: \"x\" })) \
             where Some([49]) was expected",
        ),
    ];
    for (record, detail) in wrong {
        breaks_read(&model, record, detail);
    }
    // A closed snapshot's id names no state any more.
    model
        .observe(&[
            OpRecord::SnapshotClosed { id: 0 },
            OpRecord::SnapshotClosed { id: 1 },
            read(Some(0), b"k", Ok(Some(b"7".to_vec()))),
            read(Some(1), b"k", Ok(Some(b"7".to_vec()))),
        ])
        .expect("closed snapshots are not checked");
}

/// A removal leaves its key without a value, a removal of a key without one changes nothing, an empty value is a
/// value and not a removal, and the later of two writes to one key in a batch is the one held.
#[test]
fn test_a_versionless_removal_and_the_later_write_are_what_a_read_finds() {
    let mut model = StoreModel::new();
    model
        .observe(&[
            applied(batch(vec![], &[(b"k", Some(b"1"))]), PersistMode::Buffer),
            applied(
                batch(
                    vec![],
                    &[
                        (b"k", None),
                        (b"z", None),
                        (b"e", Some(b"")),
                        (b"j", Some(b"1")),
                        (b"j", Some(b"2")),
                    ],
                ),
                PersistMode::Buffer,
            ),
        ])
        .unwrap();
    model
        .observe(&[
            read(None, b"k", Ok(None)),
            read(None, b"z", Ok(None)),
            read(None, b"e", Ok(Some(Vec::new()))),
            read(None, b"j", Ok(Some(b"2".to_vec()))),
        ])
        .expect("the removals, the empty value and the later write");
    breaks_read(
        &model,
        read(None, b"e", Ok(None)),
        "a versionless read of key 65 returned Ok(None) where Some([]) was expected",
    );
    breaks_read(
        &model,
        read(None, b"k", Ok(Some(b"1".to_vec()))),
        "a versionless read of key 6b returned Ok(Some([49])) where None was expected",
    );
    breaks_read(
        &model,
        read(None, b"j", Ok(Some(b"1".to_vec()))),
        "a versionless read of key 6a returned Ok(Some([49])) where Some([50]) was expected",
    );
}

/// A poisoned store still reads what the process sees, without the failed batch; a store whose process is down
/// refuses the read as poisoned and with nothing else, while a snapshot opened before still reads its state.
#[test]
fn test_a_versionless_read_on_a_failed_store_answers_as_its_process() {
    let mut model = StoreModel::new();
    model
        .observe(&[
            applied(batch(vec![], &[(b"k", Some(b"1"))]), PersistMode::Buffer),
            OpRecord::SnapshotOpened { id: 0, read_ts: 1 },
            OpRecord::BatchFailed {
                batch: batch(vec![], &[(b"k", Some(b"2"))]),
            },
        ])
        .unwrap();
    model
        .observe(&[read(None, b"k", Ok(Some(b"1".to_vec())))])
        .expect("the failed batch is absent until recovery");
    breaks_read(
        &model,
        read(None, b"k", Ok(Some(b"2".to_vec()))),
        "a versionless read of key 6b returned Ok(Some([50])) where Some([49]) was expected",
    );
    model.observe(&[OpRecord::Crashed]).unwrap();
    model
        .observe(&[
            read(None, b"k", Err(StoreError::Poisoned())),
            read(Some(0), b"k", Ok(Some(b"1".to_vec()))),
        ])
        .expect("the store refuses, the snapshot reads its state");
    breaks_read(
        &model,
        read(None, b"k", Ok(Some(b"1".to_vec()))),
        "a versionless read of key 6b on a store that was down returned Ok(Some([49]))",
    );
    breaks_read(
        &model,
        read(None, b"k", Err(failed(b"k"))),
        "a versionless read of key 6b on a store that was down returned \
         Err(StoreError(ReadFailed { key: [107], reason: \"x\" }))",
    );
}

/// The model saw a batch of one entry and one versionless write: contents holding one without the other, or the
/// entry beside another value, break the whole-batch invariant; contents holding both and another key break the
/// contents invariant.
#[test]
fn test_versionless_contents_other_than_the_models_break_an_invariant() {
    let both = || batch(vec![put(b"a", 1)], &[(b"k", Some(b"v"))]);
    let mut model = StoreModel::new();
    model
        .observe(&[applied(both(), PersistMode::Buffer)])
        .unwrap();
    model.clone().check(&holding(vec![both()])).unwrap();
    let wrong = [
        (
            batch(vec![put(b"a", 1)], &[]),
            Invariant::BatchWhole,
            "a batch survives with 1 of its 2 entries; the versionless write of [118] to key 6b is missing",
        ),
        (
            batch(vec![], &[(b"k", Some(b"v"))]),
            Invariant::BatchWhole,
            "a batch survives with 1 of its 2 entries; key 61 at 1 is missing",
        ),
        // The entry survives, and the key holds another value than the batch wrote: held in part.
        (
            batch(vec![put(b"a", 1)], &[(b"k", Some(b"w"))]),
            Invariant::BatchWhole,
            "a batch survives with 1 of its 2 entries; the versionless write of [118] to key 6b is missing",
        ),
        (
            batch(
                vec![put(b"a", 1)],
                &[(b"k", Some(b"v")), (b"x", Some(b"v"))],
            ),
            Invariant::ContentsMatchModel,
            "versionless key 78 holds Some([118]) where None was expected",
        ),
    ];
    for (held, invariant, detail) in wrong {
        let breach = model.clone().check(&holding(vec![held])).unwrap_err();
        assert_eq!((breach.invariant(), breach.detail()), (invariant, detail));
    }
    // A removal the store did not make: the key is still held.
    model
        .observe(&[applied(
            batch(vec![put(b"b", 2)], &[(b"k", None)]),
            PersistMode::Buffer,
        )])
        .unwrap();
    let breach = model
        .check(&holding(vec![both(), batch(vec![put(b"b", 2)], &[])]))
        .unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::BatchWhole,
            "a batch survives with 1 of its 2 entries; the versionless removal of key 6b is missing"
        )
    );
}

/// A batch that writes a key and then removes it, or a version and then deletes it, holds the later write: whole,
/// it is not reported as held in part when the contents differ elsewhere.
#[test]
fn test_a_batch_writing_one_key_twice_is_whole_with_its_later_write() {
    let twice = || {
        batch(
            vec![put(b"a", 1), Entry::deletion(b"a".to_vec(), 1)],
            &[(b"k", Some(b"v")), (b"k", None)],
        )
    };
    let mut model = StoreModel::new();
    model
        .observe(&[applied(twice(), PersistMode::Buffer)])
        .unwrap();
    model.clone().check(&holding(vec![twice()])).unwrap();
    // The store holds the batch whole and a key x the model never saw written.
    let breach = model
        .check(&holding(vec![twice(), batch(vec![put(b"x", 1)], &[])]))
        .unwrap_err();
    assert_eq!(
        breach.invariant(),
        Invariant::ContentsMatchModel,
        "{breach}"
    );
    assert!(
        breach
            .detail()
            .starts_with("key 78 holds [StoredVersion { version: 1,"),
        "{breach}"
    );
}

/// A power loss keeps a prefix of the unsynced batches, each with its versionless writes; a batch whose entries
/// and versionless writes survive apart breaks the whole-batch invariant, and the outcome adopted is what the next
/// check compares.
#[test]
fn test_a_power_loss_keeps_versionless_writes_with_their_batch() {
    // Batch i writes key "e" at version i and versionless key "v" then the digit i (hex 76 3i) to [i].
    let written = |i: u8| batch(vec![], &[(&[b'v', b'0' + i], Some(&[i]))]);
    let three: Vec<WriteBatch> = (1..=3u8)
        .map(|i| {
            let mut b = written(i);
            b.push(put(b"e", u64::from(i)));
            b
        })
        .collect();
    let lost = || {
        let mut model = StoreModel::new();
        let mut log: Vec<OpRecord> = three
            .iter()
            .map(|b| applied(b.clone(), PersistMode::Buffer))
            .collect();
        log.extend([OpRecord::PowerLost, OpRecord::Recovered]);
        model.observe(&log).unwrap();
        model
    };
    for survivors in 0..=3 {
        lost()
            .check(&holding(three[..survivors].to_vec()))
            .unwrap_or_else(|b| panic!("{survivors} survivors: {b}"));
    }
    // Batch 2's versionless write survived without its entry, and the other way round.
    let apart = [
        (
            vec![three[0].clone(), written(2)],
            "a batch survives with 1 of its 2 entries; key 65 at 2 is missing",
        ),
        (
            vec![three[0].clone(), batch(vec![put(b"e", 2)], &[])],
            "a batch survives with 1 of its 2 entries; the versionless write of [2] to key 7632 is missing",
        ),
    ];
    for (held, detail) in apart {
        let breach = lost().check(&holding(held)).unwrap_err();
        assert_eq!(
            (breach.invariant(), breach.detail()),
            (Invariant::BatchWhole, detail)
        );
    }
    // One survivor adopted: versionless key "v1" holds batch 1's value, [1], from then on.
    let mut model = lost();
    model.check(&holding(three[..1].to_vec())).unwrap();
    let breach = model
        .check(&holding(vec![batch(vec![put(b"e", 1)], &[])]))
        .unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::ContentsMatchModel,
            "versionless key 7631 holds None where Some([1]) was expected"
        )
    );
}

/// A crash with a batch in flight, or a failed batch, keeps its versionless writes with its entries, or neither.
#[test]
fn test_a_batch_in_flight_keeps_its_versionless_writes_whole_or_not_at_all() {
    let both = || batch(vec![put(b"a", 1)], &[(b"k", Some(b"v"))]);
    let failures = [
        OpRecord::CrashedDuringBatch { batch: both() },
        OpRecord::BatchFailed { batch: both() },
    ];
    for failure in failures {
        let recovered = || {
            let mut model = StoreModel::new();
            model
                .observe(&[failure.clone(), OpRecord::Recovered])
                .unwrap();
            model
        };
        recovered().check(&holding(vec![])).expect("lost whole");
        recovered()
            .check(&holding(vec![both()]))
            .expect("kept whole");
        let breach = recovered()
            .check(&holding(vec![batch(vec![], &[(b"k", Some(b"v"))])]))
            .unwrap_err();
        assert_eq!(
            (breach.invariant(), breach.detail()),
            (
                Invariant::BatchWhole,
                "a batch survives with 1 of its 2 entries; key 61 at 1 is missing"
            ),
            "{failure:?}"
        );
    }
}

/// A drop of everything clears the keyspace without versions, at once or at the recovery that finishes it; a prefix
/// drop leaves it, even a key under its prefix.
#[test]
fn test_only_a_drop_of_everything_clears_the_versionless_keys() {
    let written = || batch(vec![], &[(b"k", Some(b"v"))]);
    let kept = || holding(vec![written()]);
    let dropped = |prefixes| OpRecord::Dropped {
        ts: 2,
        prefixes,
        entries: Vec::new(),
    };
    let mut model = StoreModel::new();
    model
        .observe(&[
            applied(written(), PersistMode::Buffer),
            dropped(Some(vec![b"k".to_vec()])),
        ])
        .unwrap();
    model.check(&kept()).expect("a prefix drop keeps them");
    model.observe(&[dropped(None)]).unwrap();
    let breach = model.clone().check(&kept()).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::ContentsMatchModel,
            "versionless key 6b holds Some([118]) where None was expected"
        )
    );
    model
        .check(&Contents::default())
        .expect("a drop of everything clears them");

    // A drop of everything whose record was written but not synced, then a power loss: kept or cleared.
    let interrupted = || {
        let mut model = StoreModel::new();
        model
            .observe(&[
                applied(written(), PersistMode::Sync),
                OpRecord::DropInterrupted {
                    ts: 0,
                    prefixes: None,
                    entries: Vec::new(),
                    stage: DropStage::RecordWritten,
                    power_loss: true,
                },
                OpRecord::Recovered,
            ])
            .unwrap();
        model
    };
    interrupted().check(&kept()).expect("the record was lost");
    interrupted()
        .check(&Contents::default())
        .expect("the record survived and the drop was finished");
}

/// A ban drops a batch's entries to its namespace and none of its versionless writes, in the store and the model.
#[test]
fn test_a_ban_drops_no_versionless_write() {
    let mut key = vec![0x00];
    key.extend_from_slice(&5u64.to_be_bytes());
    key.push(b't');
    let to_five = || batch(vec![put(&key, 1)], &[(&key, Some(b"v"))]);
    let store = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    store.ban_namespace(5).unwrap();
    store.apply(to_five(), PersistMode::Buffer).unwrap();
    let mut model = StoreModel::new();
    model.observe(&store.take_log()).unwrap();
    model.check(&store.contents()).unwrap();
    let contents = store.contents();
    assert!(
        contents.keys().is_empty(),
        "the entry to namespace 5 is dropped"
    );
    assert_eq!(
        contents.versionless().iter().collect::<Vec<_>>(),
        vec![(&key, &b"v".to_vec())]
    );
    // A store that dropped the versionless write too.
    let banned = SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new());
    banned.ban_namespace(5).unwrap();
    banned
        .apply(batch(vec![put(&key, 1)], &[]), PersistMode::Buffer)
        .unwrap();
    let breach = model.check(&banned.contents()).unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::ContentsMatchModel,
            "versionless key 00000000000000000574 holds None where Some([118]) was expected"
        )
    );
}

/// After a power loss, the batch whose versionless write is missing is named by that write, even where its key holds
/// a value an earlier batch wrote, which a lost write leaves in place.
#[test]
fn test_a_versionless_write_lost_over_an_earlier_value_breaks_the_whole_batch_invariant() {
    // Batch 1 writes key "s" at 1 and versionless key "v" (hex 76) to [1], synced; batch 2 writes key "e" at 2 and
    // "v" to [2]. The store kept batch 2's entry and "v" at batch 1's [1].
    let first = || batch(vec![put(b"s", 1)], &[(b"v", Some(&[1]))]);
    let second = || batch(vec![put(b"e", 2)], &[(b"v", Some(&[2]))]);
    let mut model = StoreModel::new();
    model
        .observe(&[
            applied(first(), PersistMode::Sync),
            applied(second(), PersistMode::Buffer),
            OpRecord::PowerLost,
            OpRecord::Recovered,
        ])
        .unwrap();
    let breach = model
        .check(&holding(vec![first(), batch(vec![put(b"e", 2)], &[])]))
        .unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::BatchWhole,
            "a batch survives with 1 of its 2 entries; the versionless write of [2] to key 76 is missing"
        )
    );

    // Batches 1 to 3 each write key "e" at i and "v" to [i]. The store kept batch 1 whole and batch 2's entry, with
    // "v" still at [1]: batch 2 is held in part, and batch 3 is lost whole.
    let three: Vec<WriteBatch> = (1..=3u8)
        .map(|i| batch(vec![put(b"e", u64::from(i))], &[(b"v", Some(&[i]))]))
        .collect();
    let mut model = StoreModel::new();
    let mut log: Vec<OpRecord> = three
        .iter()
        .map(|b| applied(b.clone(), PersistMode::Buffer))
        .collect();
    log.extend([OpRecord::PowerLost, OpRecord::Recovered]);
    model.observe(&log).unwrap();
    let breach = model
        .check(&holding(vec![
            three[0].clone(),
            batch(vec![put(b"e", 2)], &[]),
        ]))
        .unwrap_err();
    assert_eq!(
        (breach.invariant(), breach.detail()),
        (
            Invariant::BatchWhole,
            "a batch survives with 1 of its 2 entries; the versionless write of [2] to key 76 is missing"
        )
    );
}
