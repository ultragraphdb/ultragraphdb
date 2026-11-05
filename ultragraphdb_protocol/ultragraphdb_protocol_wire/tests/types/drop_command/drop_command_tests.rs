/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_protocol_wire::{BodyBuilder, DropCommand, DropKind};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::commit::commit_tests::session;
use crate::utils::golden_bodies::golden;

/// The golden Drop: graph <g> of tenant 3, through the session's request 8.
pub fn golden_drop() -> DropCommand {
    DropCommand::graph(3, "http://ex.org/g").with_session(session(), 8)
}

/// The golden Drop's reply: commit version (31 << 22) | 2, the token of partition 2's round at position 42.
///
/// # Errors
///
/// When epoch 31 is refused.
pub fn golden_drop_reply() -> TestResult<DropCommand> {
    Ok(golden_drop()
        .with_commit_version((31 << 22) | 2)
        .with_token(SessionToken::new(2, 42, Epoch::new(31)?)))
}

#[test]
fn test_a_drop_builds_the_golden_frames() -> TestResult {
    assert_eq!(
        golden_drop().frame(golden("drop")?.header())?,
        golden("drop")?.bytes()?
    );
    let reply = golden_drop_reply()?;
    assert_eq!(reply.commit_version(), (31 << 22) | 2);
    assert_eq!(reply.token().map(SessionToken::position), Some(42));
    assert_eq!(
        reply.frame(golden("drop_reply")?.header())?,
        golden("drop_reply")?.bytes()?
    );
    Ok(())
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
        (DropCommand::tenant_data(3), DropKind::TenantData, 3, None),
        (DropCommand::whole_tenant(4), DropKind::Tenant, 4, None),
    ];
    for (drop, kind, tenant, target) in cases {
        assert_eq!(drop.kind(), kind);
        assert_eq!(drop.tenant(), tenant);
        assert_eq!(drop.target(), target);
        assert_eq!(
            (drop.session(), drop.request(), drop.commit_version()),
            ([0; 16], 0, 0)
        );
        assert_eq!(drop.token(), None);
        assert_eq!((drop.session(), drop.request()), ([0; 16], 0));
    }
}

#[test]
fn test_a_drop_lays_out_its_header_by_the_table() -> TestResult {
    // kind at 0, tenant at 8, session at 16, request at 32, commit_version at 40, token at 48.
    let drop = DropCommand::whole_tenant(0x0A)
        .with_session([0x0B; 16], 0x0C)
        .with_commit_version((5 << 22) | 0x0D)
        .with_token(SessionToken::new(0x0E, 0x0F, Epoch::new(5)?));
    let header = drop.command_header();
    assert_eq!(header[0], 4);
    assert_eq!(&header[1..8], &[0; 7]);
    assert_eq!(&header[8..16], &0x0Au64.to_le_bytes());
    assert_eq!(&header[16..32], &[0x0B; 16]);
    assert_eq!(&header[32..40], &0x0Cu64.to_le_bytes());
    assert_eq!(&header[40..48], &((5u64 << 22) | 0x0D).to_le_bytes());
    assert_eq!(&header[48..52], &0x0Eu32.to_le_bytes());
    assert_eq!(&header[52..56], &[0; 4]);
    assert_eq!(&header[56..64], &0x0Fu64.to_le_bytes());
    assert_eq!(&header[64..72], &5u64.to_le_bytes());
    assert!(header[72..].iter().all(|b| *b == 0));
    Ok(())
}
