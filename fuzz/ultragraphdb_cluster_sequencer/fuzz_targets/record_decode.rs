/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The sequencer's record decoder over arbitrary bytes, as a round's body read back from its log. Nothing panics;
//! bytes are decoded or refused with one of the refusals a record's decoding raises: malformed bytes, a count past its
//! limit, or a transaction naming no partition, one twice, or a reply epoch past its epoch. An accepted record encodes
//! back to exactly its bytes, the encoding having one form.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_cluster_sequencer::SequencerRecord;
use ultragraphdb_error_cluster::ClusterErrorEnum;

fn check(data: &[u8]) {
    let record = match SequencerRecord::decode(data) {
        Ok(record) => record,
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
    record.encode(&mut encoded);
    assert_eq!(encoded, data, "an accepted record re-encodes to its bytes");
}

fuzz_target!(|data: &[u8]| check(data));
