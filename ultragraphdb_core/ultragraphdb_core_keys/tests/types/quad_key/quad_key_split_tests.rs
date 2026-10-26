/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{QuadKey, QuadKeyErrorEnum, Region, SPLIT_FLAG};

use crate::types::quad_key::quad_key_tests::every_keyspace;

#[test]
fn test_a_list_key_splits_and_parses_back_with_its_start() {
    for key in every_keyspace()
        .into_iter()
        .filter(|k| k.keyspace().is_list())
    {
        for start in [1u64, 1000, u64::MAX] {
            let split = key.split_bytes(start).unwrap();
            assert_eq!(split[0], key.keyspace().byte() | SPLIT_FLAG);
            assert_eq!(&split[split.len() - 8..], &start.to_be_bytes());
            assert_eq!(
                QuadKey::parse(&split),
                Ok((key.clone(), Some(start))),
                "{key:?}"
            );
            assert_eq!(
                Region::of(&split),
                Ok(key.region()),
                "a part belongs to its list's region"
            );
            let mut trailing = split.clone();
            trailing.push(0);
            if !matches!(key, QuadKey::ValueIndex { .. }) {
                assert!(QuadKey::parse(&trailing).is_err(), "{key:?}");
            }
        }
    }
}

#[test]
fn test_only_a_list_key_splits() {
    for key in every_keyspace()
        .into_iter()
        .filter(|k| !k.keyspace().is_list())
    {
        let error = key.split_bytes(1).unwrap_err();
        assert!(
            matches!(error.0, QuadKeyErrorEnum::NotAList { .. }),
            "{key:?}"
        );
        let mut flagged = key.to_bytes();
        flagged[0] |= SPLIT_FLAG;
        flagged.extend_from_slice(&1u64.to_be_bytes());
        assert!(matches!(
            QuadKey::parse(&flagged).unwrap_err().0,
            QuadKeyErrorEnum::UnknownKeyspace { .. }
        ));
    }
}
