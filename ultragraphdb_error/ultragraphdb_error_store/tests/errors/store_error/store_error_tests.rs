/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(StoreError::TsTooOld().0, StoreErrorEnum::TsTooOld);
    assert_eq!(
        StoreError::KeyTooLarge(9, 8, vec![1]).0,
        StoreErrorEnum::KeyTooLarge {
            size: 9,
            limit: 8,
            head: vec![1]
        }
    );
    assert_eq!(
        StoreError::ReadFailed(vec![2], "r".into()).0,
        StoreErrorEnum::ReadFailed {
            key: vec![2],
            reason: "r".into()
        }
    );
    assert_eq!(
        StoreError::Corrupted(vec![3], "c".into()).0,
        StoreErrorEnum::Corrupted {
            key: vec![3],
            reason: "c".into()
        }
    );
    assert_eq!(
        StoreError::WriteFailed("w".into()).0,
        StoreErrorEnum::WriteFailed("w".into())
    );
    assert_eq!(StoreError::Poisoned().0, StoreErrorEnum::Poisoned);
    assert_eq!(
        StoreError::OpenFailed("o".into()).0,
        StoreErrorEnum::OpenFailed("o".into())
    );
    assert_eq!(
        StoreError::Unsupported("temporary keyspaces").0,
        StoreErrorEnum::Unsupported("temporary keyspaces")
    );
    assert_eq!(
        StoreError::UnknownTemporary(4).0,
        StoreErrorEnum::UnknownTemporary(4)
    );
}

#[test]
fn test_derives_compare_clone_and_hash() {
    let e = StoreError::WriteFailed("w".into());
    assert_eq!(e.clone(), e);
    assert_ne!(e, StoreError::Poisoned());
    let set: HashSet<StoreError> = [e.clone(), e].into_iter().collect();
    assert_eq!(set.len(), 1);
    assert!(format!("{:?}", StoreError::TsTooOld()).contains("TsTooOld"));
}
