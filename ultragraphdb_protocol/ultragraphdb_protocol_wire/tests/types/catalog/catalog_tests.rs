/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Catalog, Command, Header};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::CLUSTER;
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::ugs_frames::frame;

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
    let bytes = (0u32..37).map(|i| (i * 37 + 5).to_le_bytes()[0]).collect();
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
    let bytes: Vec<u8> = (0u32..37).map(|i| (i * 37 + 5).to_le_bytes()[0]).collect();
    assert_eq!(reply.bytes(), bytes.as_slice());
}

/// C1.100 at the builder: a request at a closed epoch is `version` 0 with the epoch at offset 16, the frame laid out
/// by hand.
#[test]
fn test_a_request_at_an_epoch_is_laid_out() -> TestResult {
    let request = Catalog::request_at(3, 9);
    assert_eq!(
        (
            request.tenant(),
            request.version(),
            request.epoch(),
            request.position()
        ),
        (3, 0, 9, 0)
    );
    assert!(request.bytes().is_empty());
    let built = request.frame(Header::new(CLUSTER, Command::Catalog).with_stream(3))?;
    assert_eq!(
        built,
        frame(
            Command::Catalog,
            &[(0, &3u64.to_le_bytes()), (16, &9u64.to_le_bytes())],
            &[Vec::new()],
            false
        )?
    );
    Ok(())
}
