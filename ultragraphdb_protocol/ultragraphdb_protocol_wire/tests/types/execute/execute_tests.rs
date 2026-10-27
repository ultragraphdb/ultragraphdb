/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Execute};

use crate::utils::golden_bodies::golden;

#[test]
fn test_execute_builds_the_golden_frame() {
    let execute = Execute::new(1, 0, vec!["<http://ex.org/a>".to_string()], Vec::new());
    assert_eq!(execute.statement(), 1);
    assert_eq!(execute.limit(), 0);
    assert_eq!(execute.parameters(), &["<http://ex.org/a>".to_string()]);
    assert!(execute.resume().is_empty());
    assert_eq!(
        execute.frame(golden("execute").header()).unwrap(),
        golden("execute").bytes()
    );
}
