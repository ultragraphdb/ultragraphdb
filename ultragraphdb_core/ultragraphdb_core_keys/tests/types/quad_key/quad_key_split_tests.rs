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
            want.extend_from_slice(&start.to_be_bytes());
            if let Some(first) = want.first_mut() {
                *first |= SPLIT_FLAG;
            }
            assert_eq!(
                split, want,
                "{key:?}: the flag set and the ID's 16 bytes appended"
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
            assert_eq!(split.len(), key.to_bytes().len() + 8, "{key:?}");
            assert!(split.ends_with(&fingerprint.to_be_bytes()), "{key:?}");
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
        5,
        "catalog, statements, statistics, marks, referrers"
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
fn test_a_split_start_of_neither_8_nor_16_bytes_is_refused() -> TestResult {
    let keys = list_keys()?;
    let forward = keys.first().context("the forward key")?;
    let mut base = forward.to_bytes();
    if let Some(first) = base.first_mut() {
        *first |= SPLIT_FLAG;
    }
    // Shorter than an ID's 16 bytes and not 8 is a start cut short; longer is a start with bytes after it.
    for len in (0..8).chain(9..16) {
        let mut key = base.clone();
        key.resize(base.len() + len, 0x01);
        assert_eq!(
            QuadKey::parse(&key).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::Truncated { key: key.clone() }),
            "{len}"
        );
    }
    for len in [17, 24, 32] {
        let mut key = base.clone();
        key.resize(base.len() + len, 0x01);
        assert_eq!(
            QuadKey::parse(&key).map_err(|e| e.0),
            Err(QuadKeyErrorEnum::TrailingBytes { key: key.clone() }),
            "{len}"
        );
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
