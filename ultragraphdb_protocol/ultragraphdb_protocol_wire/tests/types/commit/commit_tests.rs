/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Commit, OperationResult};

use crate::utils::golden_bodies::golden;

#[test]
fn test_commit_builds_the_golden_frame() {
    let results = vec![
        OperationResult::new(0x10, true),
        OperationResult::new(0x20, false),
    ];
    let commit = Commit::new(9, 7, 1000, results.clone());
    assert_eq!(commit.session(), 9);
    assert_eq!(commit.request(), 7);
    assert_eq!(commit.commit_version(), 1000);
    assert_eq!(commit.results(), &results[..]);
    assert_eq!(
        commit.frame(golden("commit").header()).unwrap(),
        golden("commit").bytes()
    );
}
