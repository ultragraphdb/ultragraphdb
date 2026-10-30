/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a session's entry in the keyspace without versions, its owner among them: decoded or refused without a
//! panic, and an accepted entry has one encoding.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_transactions_commit::SessionEntry;

fuzz_target!(|data: &[u8]| {
    if let Ok(entry) = SessionEntry::decode(b"fuzz", data) {
        assert_eq!(
            entry.encode().as_slice(),
            data,
            "an accepted entry has one encoding"
        );
        // Request 0 is a session registered, with no version and no results; a committed request has its version.
        assert!(entry.request() != 0 || (entry.version() == 0 && entry.results().is_empty()));
        assert!(entry.request() == 0 || entry.version() != 0);
    }
});
