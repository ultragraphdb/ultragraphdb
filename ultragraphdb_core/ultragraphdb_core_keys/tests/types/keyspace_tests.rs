/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, SPLIT_FLAG};

/// Every keyspace with its byte and whether it holds lists, compares values, names a graph and names a predicate, as
/// the quad layout states them (quad-storage, "The quad keyspaces"; the bytes are stored and never change).
const TABLE: [(Keyspace, u8, bool, bool, bool, bool); 12] = [
    (Keyspace::Catalog, 0x01, false, false, false, false),
    (Keyspace::Forward, 0x10, true, true, true, true),
    (Keyspace::Reverse, 0x11, true, true, true, true),
    (Keyspace::SubjectPredicates, 0x12, true, true, true, false),
    (Keyspace::ObjectPredicates, 0x13, true, true, true, false),
    (Keyspace::ValueIndex, 0x14, true, true, true, true),
    (Keyspace::CountIndex, 0x15, true, true, true, true),
    (Keyspace::VectorIndex, 0x16, true, false, true, true),
    (Keyspace::Statements, 0x17, false, false, true, false),
    (Keyspace::Statistics, 0x18, false, false, true, true),
    (Keyspace::Marks, 0x19, false, false, false, false),
    (Keyspace::Referrers, 0x1A, false, false, false, false),
];

#[test]
fn test_each_keyspace_has_its_byte_and_properties() {
    for (keyspace, byte, list, values, graph, predicate) in TABLE {
        assert_eq!(keyspace.byte(), byte, "{keyspace:?}");
        assert_eq!(keyspace.is_list(), list, "{keyspace:?}");
        assert_eq!(keyspace.compares_values(), values, "{keyspace:?}");
        assert_eq!(keyspace.has_graph(), graph, "{keyspace:?}");
        assert_eq!(keyspace.has_predicate(), predicate, "{keyspace:?}");
    }
}

#[test]
fn test_each_keyspace_byte_reads_back_with_and_without_the_split_flag() {
    for (keyspace, byte, ..) in TABLE {
        assert_eq!(Keyspace::from_byte(byte), Some((keyspace, false)));
        assert_eq!(
            Keyspace::from_byte(byte | SPLIT_FLAG),
            Some((keyspace, true))
        );
    }
    let known = (0u8..=0x7f).filter_map(Keyspace::from_byte).count();
    assert_eq!(known, TABLE.len(), "no other byte is a keyspace's");
    for unknown in [0x00, 0x02, 0x0F, 0x1B, 0x7F, 0x80, 0xFF] {
        assert_eq!(Keyspace::from_byte(unknown), None, "{unknown:#04x}");
    }
}
