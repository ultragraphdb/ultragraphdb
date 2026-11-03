/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::IdError;
use ultragraphdb_core_keys::{QuadKey, QuadKeyErrorEnum, Region, SPLIT_FLAG, SplitStart};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::quad_key::quad_key_tests::{every_keyspace, id};

fn list_keys() -> TestResult<Vec<QuadKey>> {
    Ok(every_keyspace()?
        .into_iter()
        .filter(|k| k.keyspace().is_list())
        .collect())
}

#[test]
fn test_a_list_key_splits_at_an_id_and_parses_back_with_it() -> TestResult {
    for key in list_keys()? {
        for start in [
            id(1, 0, 1)?,
            id(1, 5, 1000)?,
            id(u16::MAX, 0xFF_FFFF, u64::MAX)?,
        ] {
            let split = key.split_bytes(SplitStart::Id(start))?;
            let mut want = key.to_bytes();
            // The ID kind byte, 2, then the ID's 16 bytes.
            want.push(0x02);
            want.extend_from_slice(&start.to_be_bytes());
            if let Some(first) = want.first_mut() {
                *first |= SPLIT_FLAG;
            }
            assert_eq!(
                split, want,
                "{key:?}: the flag set, the ID kind byte and the ID's 16 bytes appended"
            );
            assert_eq!(
                QuadKey::parse(&split),
                Ok((key.clone(), Some(SplitStart::Id(start)))),
                "{key:?}"
            );
            assert_eq!(
                Region::of(&split),
                Ok(key.region()),
                "a part belongs to its list's region"
            );
        }
    }
    Ok(())
}

#[test]
fn test_a_list_key_but_a_value_index_key_splits_at_a_fingerprint() -> TestResult {
    for key in list_keys()?
        .into_iter()
        .filter(|k| !matches!(k, QuadKey::ValueIndex { .. }))
    {
        for fingerprint in [0u64, 1, 0x0123_4567_89AB_CDEF, u64::MAX] {
            let start = SplitStart::Fingerprint(fingerprint);
            let split = key.split_bytes(start)?;
            // The fingerprint kind byte, 1, then the fingerprint's 8 bytes.
            assert_eq!(split.len(), key.to_bytes().len() + 9, "{key:?}");
            let mut tail = vec![0x01];
            tail.extend_from_slice(&fingerprint.to_be_bytes());
            assert!(split.ends_with(&tail), "{key:?}");
            assert_eq!(
                QuadKey::parse(&split),
                Ok((key.clone(), Some(start))),
                "{key:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_a_value_index_key_splits_at_an_id_alone() -> TestResult {
    for key in list_keys()?
        .into_iter()
        .filter(|k| matches!(k, QuadKey::ValueIndex { .. }))
    {
        let Err(error) = key.split_bytes(SplitStart::Fingerprint(7)) else {
            return Err(TestError::new("a fingerprint start is refused"));
        };
        assert_eq!(
            error.0,
            QuadKeyErrorEnum::FingerprintStart {
                key: key.to_bytes()
            }
        );
    }
    Ok(())
}

#[test]
fn test_only_a_list_key_splits() -> TestResult {
    let non_lists: Vec<QuadKey> = every_keyspace()?
        .into_iter()
        .filter(|k| !k.keyspace().is_list())
        .collect();
    assert_eq!(
        non_lists.len(),
        8,
        "catalog, statements, statistics, marks, referrers, three dictionary entries"
    );
    for key in non_lists {
        let Err(error) = key.split_bytes(SplitStart::Id(id(1, 1, 1)?)) else {
            return Err(TestError::new(
                "split_bytes returned Ok where an error was expected",
            ));
        };
        assert_eq!(
            error.0,
            QuadKeyErrorEnum::NotAList {
                key: key.to_bytes()
            }
        );
        let mut flagged = key.to_bytes();
        if let Some(first) = flagged.first_mut() {
            *first |= SPLIT_FLAG;
        }
        flagged.extend_from_slice(&id(1, 1, 1)?.to_be_bytes());
        assert_eq!(
            QuadKey::parse(&flagged).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::UnknownKeyspace {
                key: flagged.clone()
            })
        );
    }
    Ok(())
}

#[test]
fn test_a_split_start_cut_short_or_running_on_is_refused() -> TestResult {
    let keys = list_keys()?;
    let forward = keys.first().context("the forward key")?;
    let mut base = forward.to_bytes();
    if let Some(first) = base.first_mut() {
        *first |= SPLIT_FLAG;
    }
    let refused = |key: &[u8]| QuadKey::parse(key).map_err(|e| e.0);
    // The flag with no kind byte after the fields.
    assert_eq!(
        refused(&base),
        Err(QuadKeyErrorEnum::Truncated { key: base.clone() })
    );
    // Kind 1 takes a fingerprint's 8 bytes and kind 2 an ID's 16: fewer is a start cut short, more a start with bytes
    // after it.
    let id_start = id(1, 5, 1000)?.to_be_bytes().to_vec();
    for (kind, start) in [(0x01u8, vec![0x01; 8]), (0x02, id_start)] {
        let len = start.len();
        for given in 0..len {
            let mut key = base.clone();
            key.push(kind);
            key.extend_from_slice(start.get(..given).context("a cut start")?);
            assert_eq!(
                refused(&key),
                Err(QuadKeyErrorEnum::Truncated { key: key.clone() }),
                "{kind} {given}"
            );
        }
        for given in [len + 1, len + 8, 32] {
            let mut key = base.clone();
            key.push(kind);
            key.extend_from_slice(&start);
            key.resize(base.len() + 1 + given, 0x01);
            assert_eq!(
                refused(&key),
                Err(QuadKeyErrorEnum::TrailingBytes { key: key.clone() }),
                "{kind} {given}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_an_id_start_cut_by_8_bytes_is_no_fingerprint_start() -> TestResult {
    // Hand-computed: forward key, namespace 133, the default graph, predicate (type 1, bucket 7, sequence 7), subject
    // (type 1, bucket 42, sequence 42), split at the ID (type 1, bucket 5, sequence 1000): flag 0x90, the namespace,
    // 16 zero bytes, the two IDs, kind 2 and the start's 16 bytes.
    let key = QuadKey::Forward {
        namespace: 133,
        graph: ultragraphdb_core_keys::DEFAULT_GRAPH,
        predicate: id(1, 7, 7)?,
        subject: id(1, 42, 42)?,
    };
    let mut want = vec![0x90, 0, 0, 0, 0, 0, 0, 0, 0x85];
    want.extend_from_slice(&[0; 16]);
    want.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 7, 0, 0, 0]);
    want.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 42, 0, 0, 0]);
    want.extend_from_slice(&[0x02, 0, 1, 0, 0, 0, 0, 0, 0, 0x03, 0xE8, 0, 0, 5, 0, 0, 0]);
    let split = key.split_bytes(SplitStart::Id(id(1, 5, 1000)?))?;
    assert_eq!(split, want);
    // Its last 8 bytes cut: kind 2 still names an ID, so the key is cut short, never a key split at a fingerprint.
    let cut = split.get(..split.len() - 8).context("the cut key")?;
    assert_eq!(
        QuadKey::parse(cut).map_err(|e| e.0),
        Err(QuadKeyErrorEnum::Truncated { key: cut.to_vec() })
    );
    Ok(())
}

#[test]
fn test_split_keys_of_one_list_sort_by_their_starts_as_posting_keys() -> TestResult {
    // A posting key is 128 bits: a fingerprint below 2^64 sorts before every ID, whose type prefix is never 0.
    let keys = list_keys()?;
    let forward = keys.first().context("the forward key")?;
    let starts = [
        SplitStart::Fingerprint(0),
        SplitStart::Fingerprint(u64::MAX),
        SplitStart::Id(id(1, 0, 1)?),
        SplitStart::Id(id(1, 5, 1000)?),
        SplitStart::Id(id(2, 0, 1)?),
    ];
    let bytes = starts
        .iter()
        .map(|s| forward.split_bytes(*s))
        .collect::<Result<Vec<_>, _>>()?;
    assert!(bytes.is_sorted());
    Ok(())
}

#[test]
fn test_an_unknown_start_kind_is_refused() -> TestResult {
    let keys = list_keys()?;
    let forward = keys.first().context("the forward key")?;
    for kind in [0x00u8, 0x03, 0xFF] {
        let mut key = forward.split_bytes(SplitStart::Id(id(1, 1, 1)?))?;
        let at = forward.to_bytes().len();
        *key.get_mut(at).context("the kind byte")? = kind;
        assert_eq!(
            QuadKey::parse(&key).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::UnknownKind {
                key: key.clone(),
                kind
            })
        );
    }
    Ok(())
}

#[test]
fn test_a_value_index_key_reads_its_start_from_its_last_17_bytes() -> TestResult {
    for key in list_keys()?
        .into_iter()
        .filter(|k| matches!(k, QuadKey::ValueIndex { .. }))
    {
        let split = key.split_bytes(SplitStart::Id(id(1, 5, 1000)?))?;
        let at = split.len() - 17;
        // Kind 1 is a fingerprint, which a value-index key never splits at; any other kind but 2 names nothing.
        for kind in [0x01u8, 0x07] {
            let mut bad = split.clone();
            *bad.get_mut(at).context("the kind byte")? = kind;
            let want = if kind == 0x01 {
                QuadKeyErrorEnum::FingerprintStart { key: bad.clone() }
            } else {
                QuadKeyErrorEnum::UnknownKind {
                    key: bad.clone(),
                    kind,
                }
            };
            assert_eq!(QuadKey::parse(&bad).map_err(|e| e.0), Err(want), "{key:?}");
        }
    }
    Ok(())
}

#[test]
fn test_a_split_start_that_is_no_valid_id_is_refused() -> TestResult {
    for key in list_keys()? {
        let mut split = key.split_bytes(SplitStart::Id(id(1, 1, 1)?))?;
        let len = split.len();
        // The start's sequence, bytes 2 to 9 of its 16, all zero.
        split
            .get_mut(len - 14..len - 6)
            .context("the start's sequence")?
            .fill(0);
        assert_eq!(
            QuadKey::parse(&split).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::InvalidId {
                key: split.clone(),
                field: "split start",
                error: IdError::zero_sequence()
            }),
            "{key:?}"
        );
    }
    Ok(())
}

#[test]
fn test_a_split_flag_with_no_room_for_a_start_is_truncated() -> TestResult {
    // The flag alone, and a value-index key shorter than its head and a 16-byte start.
    for key in [vec![0x90u8], vec![0x94; 30]] {
        assert_eq!(
            QuadKey::parse(&key).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::Truncated { key: key.clone() })
        );
    }
    Ok(())
}
