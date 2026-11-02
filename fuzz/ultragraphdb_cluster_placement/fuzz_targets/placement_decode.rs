/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The placement map's decoder over arbitrary bytes. Nothing panics; bytes are decoded or refused as malformed, the
//! one error a decoder raises. An accepted map encodes back to exactly its bytes, the encoding having one form, and
//! the layout of its latest change covers every bucket once, in ranges that follow each other from bucket 0 to the
//! end, each resolving to its own partition.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_cluster_placement::{BucketRange, PlacementMap};
use ultragraphdb_core_ids::Bucket;
use ultragraphdb_error_cluster::ClusterErrorEnum;

fn check(data: &[u8]) {
    let map = match PlacementMap::decode(data) {
        Ok(map) => map,
        Err(error) => {
            assert!(
                matches!(error.0, ClusterErrorEnum::Malformed { offset, .. } if offset <= data.len()),
                "{error:?}"
            );
            return;
        }
    };
    let mut encoded = Vec::with_capacity(data.len());
    map.encode(&mut encoded);
    assert_eq!(encoded, data, "an accepted map re-encodes to its bytes");
    let epoch = map.latest_change();
    let mut next = 0;
    for (range, partition) in map.ranges_at(epoch) {
        assert_eq!(range.start(), next, "ranges follow each other");
        let first = Bucket::new(range.start()).map(|bucket| map.partition_of(bucket, epoch));
        assert_eq!(first, Ok(partition), "a range's first bucket resolves to its partition");
        next = range.end();
    }
    assert_eq!(next, BucketRange::END, "the ranges cover every bucket");
}

fuzz_target!(|data: &[u8]| check(data));
