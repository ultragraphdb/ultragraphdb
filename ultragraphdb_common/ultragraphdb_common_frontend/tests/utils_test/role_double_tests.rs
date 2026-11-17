/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_frontend::{RoleDouble, committed, partition_answer, refusal_of};
use ultragraphdb_common_role::{RoleCluster, RoleSecret};
use ultragraphdb_protocol_transport::Endpoint;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, CommitView, ErrorCode, ErrorReply, FrameView, Header,
    MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::TestResult;

const CLUSTER: u64 = 7;
const DETAIL: u64 = 3;

fn cluster() -> TestResult<RoleCluster> {
    let file = std::env::temp_dir().join(format!("role-double-{}", std::process::id()));
    std::fs::write(&file, "a role secret")?;
    Ok(RoleCluster::new(
        CLUSTER,
        RoleSecret::read(&file.display().to_string())?,
        Duration::from_secs(1),
    ))
}

/// A double serves the cluster it was started for.
#[test]
fn test_a_double_serves_its_cluster() -> TestResult {
    let cluster = cluster()?;
    Runtime::new()?.block_on(async {
        let double = RoleDouble::start(&cluster, Endpoint::Partition, |frame| {
            partition_answer(frame, CLUSTER, 1)
        })
        .await?;
        assert_eq!(double.cluster(), cluster.id());
        Ok(())
    })
}

/// The refusal an Error frame holds is read back whole.
#[test]
fn test_an_error_frames_refusal_is_read_back() -> TestResult {
    let refusal = ErrorReply::new(ErrorCode::Refused, "a refusal").with_detail(DETAIL);
    let frame = refusal.frame(Header::new(CLUSTER, Command::Error).with_stream(1))?;
    let read = refusal_of(&frame, CLUSTER)?;
    assert_eq!(
        (read.code(), read.detail(), read.message()),
        (refusal.code(), refusal.detail(), refusal.message())
    );
    Ok(())
}

/// A Commit reply keeps its rules (`PROTOCOL.md` §6.3): through a session it carries a token of its version's epoch, the
/// version's bits above its 22 low ones; through none, no token.
#[test]
fn test_a_commit_reply_carries_a_token_through_a_session_alone() -> TestResult {
    // Epoch 5 is the version's bits above its low 22.
    let version = (5 << 22) | 9;
    // A request numbered through a session, 0 through none.
    for (session, request, epoch) in [([3; 16], 1, Some(5)), ([0; 16], 0, None)] {
        let frame =
            committed(session, request, version).frame(Header::new(CLUSTER, Command::Commit))?;
        let reply = CommitView::from_frame(&FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?)?;
        assert_eq!(reply.commit_version(), version);
        assert_eq!(reply.token().map(|token| token.epoch().get()), epoch);
    }
    Ok(())
}
