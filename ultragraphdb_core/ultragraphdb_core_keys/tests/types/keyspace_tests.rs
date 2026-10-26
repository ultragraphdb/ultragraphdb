/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, SPLIT_FLAG};

#[test]
fn test_each_keyspace_byte_reads_back_with_and_without_the_split_flag() {
    let all = (0u8..=0x7f)
        .filter_map(Keyspace::from_byte)
        .collect::<Vec<_>>();
    assert_eq!(all.len(), 10);
    for (keyspace, split) in all {
        assert!(!split);
        assert_eq!(
            Keyspace::from_byte(keyspace.byte()),
            Some((keyspace, false))
        );
        assert_eq!(
            Keyspace::from_byte(keyspace.byte() | SPLIT_FLAG),
            Some((keyspace, true))
        );
    }
    assert_eq!(Keyspace::from_byte(0x00), None);
    assert!(
        Keyspace::Forward.is_list()
            && !Keyspace::Statements.is_list()
            && Keyspace::VectorIndex.is_list()
    );
    assert!(Keyspace::Statistics.has_predicate() && !Keyspace::ObjectPredicates.has_predicate());
    assert!(
        Keyspace::Forward.compares_values()
            && !Keyspace::VectorIndex.compares_values()
            && !Keyspace::Statistics.compares_values()
    );
}
