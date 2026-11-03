/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata group's snapshot decoder over arbitrary bytes, as a stored snapshot read back from the disk. Nothing
//! panics; bytes are decoded or refused as malformed or as a count past its table's limit. An accepted snapshot encodes
//! back to exactly its bytes, the encoding having one form.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_cluster_metadata::MetadataSnapshot;
use ultragraphdb_error_cluster::ClusterErrorEnum;

fn check(data: &[u8]) {
    let snapshot = match MetadataSnapshot::decode(data) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let typed = match error.0 {
                ClusterErrorEnum::Malformed { offset, .. } => offset <= data.len(),
                ClusterErrorEnum::TooMany { .. } => true,
                _ => false,
            };
            assert!(typed, "{error:?}");
            return;
        }
    };
    let mut encoded = Vec::with_capacity(data.len());
    snapshot.encode(&mut encoded);
    assert_eq!(
        encoded, data,
        "an accepted snapshot re-encodes to its bytes"
    );
}

fuzz_target!(|data: &[u8]| check(data));
