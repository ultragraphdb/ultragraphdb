/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{DEFAULT_GRAPH, Graph};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::quad_key::quad_key_tests::id;

#[test]
fn test_the_default_graph_is_16_zero_bytes() {
    assert_eq!(DEFAULT_GRAPH, Graph::Default);
    assert_eq!(Graph::Default.to_be_bytes(), [0u8; 16]);
}

#[test]
fn test_a_named_graph_is_its_ids_bytes() -> TestResult {
    // (type 3, bucket 0xAB_CDEF, sequence 42): 00 03 | 00 00 00 00 00 00 00 2A | AB CD EF | 00 00 00, by hand.
    let bytes = [
        0x00, 0x03, 0, 0, 0, 0, 0, 0, 0, 0x2A, 0xAB, 0xCD, 0xEF, 0, 0, 0,
    ];
    assert_eq!(Graph::Named(id(3, 0xAB_CDEF, 42)?).to_be_bytes(), bytes);
    Ok(())
}

#[test]
fn test_the_default_graph_sorts_before_every_named_graph() -> TestResult {
    assert!(Graph::Default < Graph::Named(id(1, 0, 1)?));
    assert!(Graph::Named(id(1, 0, 1)?) < Graph::Named(id(1, 0, 2)?));
    Ok(())
}
