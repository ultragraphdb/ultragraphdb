/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Join};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden storage Join.
pub fn golden_join_storage() -> Join {
    Join::storage("tcp://storage-1:7300", vec![2, 5, 9])
}

#[test]
fn test_the_builder_lays_out_the_golden_join_storage() -> TestResult {
    let golden = golden_role("join_storage")?;
    assert_eq!(
        golden_join_storage().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

/// The golden compute Join.
pub fn golden_join_compute() -> Join {
    Join::compute("unix:///run/ugdb/compute.sock")
}

#[test]
fn test_the_builder_lays_out_the_golden_join_compute() -> TestResult {
    let golden = golden_role("join_compute")?;
    assert_eq!(
        golden_join_compute().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}
