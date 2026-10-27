/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a superblock copy: decoded or refused without a panic, and an accepted record has one encoding.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_transactions_log::Superblock;

fuzz_target!(|data: &[u8]| {
    if let Some(record) = Superblock::decode(data) {
        assert_eq!(record.encode().as_slice(), data, "an accepted record has one encoding");
        assert!(record.copy() == 'a' || record.copy() == 'b');
    }
});
