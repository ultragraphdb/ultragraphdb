/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The posting format's decoder over arbitrary bytes. It returns a list or the store's `Corrupted` naming the key,
//! and never panics; a list it accepts encodes back to the bytes it was decoded from.
//!
//! Random bytes almost never carry a valid checksum, so each input is decoded twice: as given, which exercises the
//! checks before the checksum, and with its last eight bytes replaced by the checksum of the rest, which reaches the
//! parse behind it.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_codec::PostingList;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use xxhash_rust::xxh3::xxh3_64;

const KEY: &[u8] = b"\x01fuzz";

fn check(bytes: &[u8]) {
    match PostingList::decode(KEY, bytes) {
        Ok(plist) => assert_eq!(plist.encode(), bytes, "an accepted list encodes back to its bytes"),
        Err(StoreError(StoreErrorEnum::Corrupted { key, .. })) => assert_eq!(key, KEY),
        Err(other) => panic!("not refused as corruption: {other:?}"),
    }
}

fuzz_target!(|data: &[u8]| {
    check(data);
    if data.len() >= 8 {
        let mut forged = data.to_vec();
        let body = forged.len() - 8;
        let checksum = xxh3_64(&forged[..body]);
        forged[body..].copy_from_slice(&checksum.to_le_bytes());
        check(&forged);
    }
});
