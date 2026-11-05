/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::TenantRun;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_run_encodes_tenant_and_run() -> TestResult {
    let run = TenantRun::new(0x0102_0304_0506_0708, 0xFE);
    assert_eq!((run.tenant(), run.run()), (0x0102_0304_0506_0708, 0xFE));
    assert_eq!(
        run.encode(),
        [8, 7, 6, 5, 4, 3, 2, 1, 0xFE, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(TenantRun::SIZE, 16);
    Ok(())
}
