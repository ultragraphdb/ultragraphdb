/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Catalog};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden Catalog request.
pub fn golden_catalog() -> Catalog {
    Catalog::request(3, 0)
}

#[test]
fn test_the_builder_lays_out_the_golden_catalog() -> TestResult {
    let golden = golden_role("catalog")?;
    assert_eq!(
        golden_catalog().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

/// The golden Catalog reply.
pub fn golden_catalog_reply() -> Catalog {
    let bytes = (0u32..37).map(|i| ((i * 37 + 5) % 256) as u8).collect();
    Catalog::reply(3, 12, 44, 1_240, bytes)
}

#[test]
fn test_the_builder_lays_out_the_golden_catalog_reply() -> TestResult {
    let golden = golden_role("catalog_reply")?;
    assert_eq!(
        golden_catalog_reply().frame(golden.header())?,
        role_bytes(&golden)?
    );
    Ok(())
}

#[test]
fn test_the_getters_give_back_what_the_constructors_took() {
    let request = golden_catalog();
    assert_eq!(
        (
            request.tenant(),
            request.version(),
            request.epoch(),
            request.position()
        ),
        (3, 0, 0, 0)
    );
    assert!(request.bytes().is_empty());
    let reply = golden_catalog_reply();
    assert_eq!(
        (
            reply.tenant(),
            reply.version(),
            reply.epoch(),
            reply.position()
        ),
        (3, 12, 44, 1_240)
    );
    let bytes: Vec<u8> = (0u32..37).map(|i| ((i * 37 + 5) % 256) as u8).collect();
    assert_eq!(reply.bytes(), bytes.as_slice());
}
