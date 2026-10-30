/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, DropCommand, DropKind};

use crate::utils::golden_bodies::golden;

/// The golden Drop: graph <g> of namespace 3, through session 9's request 8.
pub fn golden_drop() -> DropCommand {
    DropCommand::graph(3, "http://ex.org/g").with_session(9, 8)
}

#[test]
fn test_a_drop_builds_the_golden_frames() {
    assert_eq!(
        golden_drop().frame(golden("drop").header()).unwrap(),
        golden("drop").bytes()
    );
    let reply = golden_drop().with_commit_version(2002);
    assert_eq!(reply.commit_version(), 2002);
    assert_eq!(
        reply.frame(golden("drop_reply").header()).unwrap(),
        golden("drop_reply").bytes()
    );
}

#[test]
fn test_each_kind_has_its_constructor() {
    let cases = [
        (
            DropCommand::predicate(1, "http://ex.org/p"),
            DropKind::Predicate,
            1,
            Some("http://ex.org/p"),
        ),
        (
            DropCommand::graph(2, "http://ex.org/g"),
            DropKind::Graph,
            2,
            Some("http://ex.org/g"),
        ),
        (
            DropCommand::namespace_data(3),
            DropKind::NamespaceData,
            3,
            None,
        ),
        (
            DropCommand::whole_namespace(4),
            DropKind::Namespace,
            4,
            None,
        ),
        (DropCommand::all(), DropKind::All, 0, None),
    ];
    for (drop, kind, namespace, target) in cases {
        assert_eq!(drop.kind(), kind);
        assert_eq!(drop.namespace(), namespace);
        assert_eq!(drop.target(), target);
        assert_eq!(
            (drop.session(), drop.request(), drop.commit_version()),
            (0, 0, 0)
        );
    }
}

#[test]
fn test_a_drop_lays_out_its_header_by_the_table() {
    // kind at 0, namespace at 8, session at 16, request at 24, commit_version at 32.
    let drop = DropCommand::whole_namespace(0x0A)
        .with_session(0x0B, 0x0C)
        .with_commit_version(0x0D);
    let header = drop.command_header();
    assert_eq!(header[0], 4);
    assert_eq!(&header[1..8], &[0; 7]);
    assert_eq!(&header[8..16], &0x0Au64.to_le_bytes());
    assert_eq!(&header[16..24], &0x0Bu64.to_le_bytes());
    assert_eq!(&header[24..32], &0x0Cu64.to_le_bytes());
    assert_eq!(&header[32..40], &0x0Du64.to_le_bytes());
    assert!(header[40..].iter().all(|b| *b == 0));
}
