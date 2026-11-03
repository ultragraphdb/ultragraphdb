/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::NamespaceRun;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_run_encodes_namespace_and_run() -> TestResult {
    let run = NamespaceRun::new(0x0102_0304_0506_0708, 0xFE);
    assert_eq!((run.namespace(), run.run()), (0x0102_0304_0506_0708, 0xFE));
    assert_eq!(
        run.encode(),
        [8, 7, 6, 5, 4, 3, 2, 1, 0xFE, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(NamespaceRun::SIZE, 16);
    Ok(())
}
