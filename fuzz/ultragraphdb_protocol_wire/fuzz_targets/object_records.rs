/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a list's value (`PROTOCOL.md` §13.14): the reader never panics, and a value it accepts holds the records
//! it counted and encodes to the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{ObjectRecords, ObjectRecordsBuilder};

fuzz_target!(|data: &[u8]| {
    let Ok(records) = ObjectRecords::read(data) else {
        return;
    };
    let mut rebuilt = ObjectRecordsBuilder::new();
    let mut count = 0;
    for record in records.iter() {
        let statements: Vec<[u8; 16]> = record.statement_ids().collect();
        assert_eq!(statements.len(), record.statement_count());
        rebuilt.push(record.encoding(), &statements);
        count += 1;
    }
    assert_eq!(count, records.len());
    assert_eq!(rebuilt.value(), data, "a value has one encoding");
});
