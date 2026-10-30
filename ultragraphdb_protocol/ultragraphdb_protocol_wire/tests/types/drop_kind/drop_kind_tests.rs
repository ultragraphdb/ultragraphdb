/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::DropKind;

#[test]
fn test_drop_kinds_keep_their_numbers() {
    let numbers: Vec<u8> = DropKind::ALL.iter().map(|k| k.number()).collect();
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);
    assert_eq!(DropKind::Predicate.number(), 1);
    assert_eq!(DropKind::Graph.number(), 2);
    assert_eq!(DropKind::NamespaceData.number(), 3);
    assert_eq!(DropKind::Namespace.number(), 4);
    assert_eq!(DropKind::All.number(), 5);
    for kind in DropKind::ALL {
        assert_eq!(DropKind::from_number(kind.number()), Some(kind));
    }
    assert_eq!(DropKind::from_number(0), None);
    assert_eq!(DropKind::from_number(6), None);
    assert_eq!(DropKind::from_number(u8::MAX), None);
}

#[test]
fn test_a_predicate_and_a_graph_drop_name_their_target() {
    let named: Vec<DropKind> = DropKind::ALL
        .into_iter()
        .filter(|k| k.names_target())
        .collect();
    assert_eq!(named, vec![DropKind::Predicate, DropKind::Graph]);
}
