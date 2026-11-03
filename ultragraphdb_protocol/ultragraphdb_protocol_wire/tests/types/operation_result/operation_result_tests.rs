/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{ABSENT, OperationResult};

/// A result's 16 bytes laid out from PROTOCOL.md §6.3's table: `value` i64, `node` u32, `created` u8, 3 zero bytes.
fn layout(value: i64, node: u32, created: u8) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&value.to_le_bytes());
    out[8..12].copy_from_slice(&node.to_le_bytes());
    out[12] = created;
    out
}

#[test]
fn test_a_node_result_names_its_term_and_whether_it_created() {
    for (node, created) in [(0u32, true), (1, false), (ABSENT - 1, true)] {
        let result = OperationResult::for_node(node, created);
        assert_eq!(result.node(), Some(node));
        assert_eq!(result.created(), created);
        assert_eq!(result.value(), 0);
        assert_eq!(result.encode(), layout(0, node, u8::from(created)));
    }
    assert_ne!(
        OperationResult::for_node(3, true),
        OperationResult::for_node(3, false)
    );
    assert_ne!(
        OperationResult::for_node(3, false),
        OperationResult::for_node(4, false)
    );
}

#[test]
fn test_a_counters_result_holds_its_new_value_and_no_node() {
    for value in [1i64, -1, -5, i64::MIN, i64::MAX] {
        let result = OperationResult::counter(value);
        assert_eq!(result.value(), value);
        assert_eq!(result.node(), None);
        assert!(!result.created());
        assert_eq!(result.encode(), layout(value, ABSENT, 0));
    }
    // -1 in two's complement: eight 0xFF bytes.
    assert_eq!(OperationResult::counter(-1).encode()[..8], [0xFF; 8]);
}

#[test]
fn test_a_compare_and_set_holds_nothing() {
    let result = OperationResult::compare_and_set();
    assert_eq!(result.value(), 0);
    assert_eq!(result.node(), None);
    assert!(!result.created());
    assert_eq!(result.encode(), layout(0, ABSENT, 0));
    // A counter added to 0 has the same bytes: which operation it answers is the request's.
    assert_eq!(OperationResult::counter(0), result);
    assert_ne!(OperationResult::for_node(0, false), result);
}
