/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_frontend::{RoleDouble, partition_answer, refusal_of};
use ultragraphdb_common_role::{RoleCluster, RoleSecret};
use ultragraphdb_protocol_transport::Endpoint;
use ultragraphdb_protocol_wire::{BodyBuilder, Command, ErrorCode, ErrorReply, Header};
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
