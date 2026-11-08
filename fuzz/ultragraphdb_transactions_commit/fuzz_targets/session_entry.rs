/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a session's entry in the keyspace without versions, its stored reply's token, results and node texts
//! among them: decoded or refused without a panic, and an accepted entry has one encoding.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_transactions_commit::SessionEntry;

fuzz_target!(|data: &[u8]| {
    if let Ok(entry) = SessionEntry::decode(b"fuzz", data) {
        // An accepted entry's terms are IRIs, so it encodes, and to the bytes it was read from.
        assert_eq!(
            entry.encode().as_deref(),
            Ok(data),
            "an accepted entry has one encoding"
        );
        // A stored reply is a commit's: a request above 0 and an even commit version above 0.
        assert!(entry.request() != 0);
        assert!(entry.reply().version() != 0 && entry.reply().version() % 2 == 0);
        // Every result naming a node names one the reply's term table holds.
        let nodes = entry.reply().terms().len();
        assert!(
            entry
                .reply()
                .results()
                .iter()
                .filter_map(|r| r.node())
                .all(|node| (node as usize) < nodes)
        );
    }
});
