/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a superblock copy: decoded or refused without a panic, and an accepted record has one encoding. Then,
//! from the other side, a record of the input's first 48 bytes as its six fields, little-endian, encodes to bytes
//! that decode to it: random bytes almost never carry the checksum, which this reaches without forging it.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::Superblock;

fuzz_target!(|data: &[u8]| {
    if let Some(record) = Superblock::decode(data) {
        assert_eq!(
            record.encode().as_slice(),
            data,
            "an accepted record has one encoding"
        );
        assert!(record.copy() == 'a' || record.copy() == 'b');
    }
    if let Some(fields) = data.get(..48) {
        let field = |i: usize| {
            u64::from_le_bytes(fields[8 * i..8 * i + 8].try_into().expect("eight bytes"))
        };
        let record = Superblock::new(
            field(0),
            field(1),
            field(2),
            Position::new(field(3)),
            field(4),
            field(5),
        );
        assert_eq!(
            Superblock::decode(&record.encode()),
            Some(record),
            "an encoded record decodes to itself"
        );
    }
});
