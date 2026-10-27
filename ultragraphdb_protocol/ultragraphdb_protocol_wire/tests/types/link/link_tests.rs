/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Link;

use crate::types::write::write_tests::ex;

#[test]
fn test_a_link_names_its_direction() {
    let from = Link::from_node(ex("knows"), ex("b").into());
    assert!(from.node_is_subject());
    assert_eq!(from.predicate(), &ex("knows"));
    assert_eq!(from.other(), &ex("b").into());
    let to = Link::to_node(ex("knows"), ex("b").into());
    assert!(!to.node_is_subject());
    assert_ne!(from, to);
}
