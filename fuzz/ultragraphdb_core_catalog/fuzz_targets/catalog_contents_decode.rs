/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A catalog version's contents decoder over arbitrary bytes, read as the namespace and version their fixed part
//! names, and as another version: decoded, or refused as a corrupted version naming the namespace and version asked
//! for; nothing panics. Accepted contents encode back to the bytes they were decoded from, and are refused as any
//! other version.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_catalog::{CatalogContents, CatalogErrorEnum};

/// The u64 at `at` of the contents' fixed part, 0 when the bytes end before it.
fn field(data: &[u8], at: usize) -> u64 {
    data.get(at..at + 8)
        .and_then(|b| <[u8; 8]>::try_from(b).ok())
        .map_or(0, u64::from_be_bytes)
}

fn check(data: &[u8]) {
    let (namespace, version) = (field(data, 1), field(data, 9));
    match CatalogContents::decode(namespace, version, data) {
        Ok(contents) => {
            assert_eq!(contents.namespace(), namespace);
            assert_eq!(contents.version(), version);
            let encoded = contents.encode();
            assert_eq!(
                encoded.as_deref(),
                Ok(data),
                "accepted contents encode back to their bytes"
            );
            let other = CatalogContents::decode(namespace, version.wrapping_add(1), data);
            assert!(other.is_err(), "contents name their own version");
        }
        Err(error) => match error.0 {
            CatalogErrorEnum::CorruptedVersion {
                namespace: n,
                version: v,
                ..
            } => assert_eq!((n, v), (namespace, version)),
            other => panic!("refused with another error: {other:?}"),
        },
    }
}

fuzz_target!(|data: &[u8]| check(data));
