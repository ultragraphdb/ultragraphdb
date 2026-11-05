/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Install};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

/// The golden Install's sources: a schema and a module.
pub fn sources() -> Vec<(String, String)> {
    [
        ("crm", "namespace crm {}"),
        ("crm/people", "module people {}"),
    ]
    .map(|(name, text)| (name.to_string(), text.to_string()))
    .to_vec()
}

#[test]
fn test_a_request_and_its_reply_build_the_golden_frames() -> TestResult {
    let request = Install::new(3, 11, sources());
    assert_eq!((request.tenant(), request.version()), (3, 11));
    assert_eq!(request.epoch(), 0);
    assert_eq!(request.sources(), sources());
    assert_eq!(request.report(), "");
    assert_eq!(
        request.frame(golden("install")?.header())?,
        golden("install")?.bytes()?
    );
    let report = "coordination-free: 2";
    let reply = request.installed(12, 44, report);
    assert_eq!((reply.version(), reply.epoch()), (12, 44));
    assert_eq!(reply.report(), report);
    assert_eq!(
        reply.frame(golden("install_reply")?.header())?,
        golden("install_reply")?.bytes()?
    );
    Ok(())
}
