/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Link;

#[test]
fn test_a_link_names_its_direction() {
    let from = Link::from_node(4, 5);
    assert!(from.node_is_subject());
    assert_eq!((from.predicate(), from.other()), (4, 5));
    let to = Link::to_node(4, 5);
    assert!(!to.node_is_subject());
    assert_eq!((to.predicate(), to.other()), (4, 5));
    assert_ne!(from, to);
}
