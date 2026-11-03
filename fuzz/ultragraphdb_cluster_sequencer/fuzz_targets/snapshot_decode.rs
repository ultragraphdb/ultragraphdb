/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The sequencer's snapshot decoder over arbitrary bytes, as a stored snapshot read back from the disk. Nothing panics;
//! bytes are decoded or refused with one of the refusals a snapshot's decoding raises: malformed bytes, a count past
//! its limit, or a record's own refusals. An accepted snapshot encodes back to exactly its bytes, the encoding having
//! one form.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_cluster_sequencer::SequencerSnapshot;
use ultragraphdb_error_cluster::ClusterErrorEnum;

fn check(data: &[u8]) {
    let snapshot = match SequencerSnapshot::decode(data) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let typed = match error.0 {
                ClusterErrorEnum::Malformed { offset, .. } => offset <= data.len(),
                ClusterErrorEnum::TooMany { .. }
                | ClusterErrorEnum::NoPartitions
                | ClusterErrorEnum::DuplicatePartition { .. }
                | ClusterErrorEnum::EpochAhead { .. } => true,
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
