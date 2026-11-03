/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyView, Catalog, CatalogView, MAX_EPOCH};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::catalog::catalog_tests::{golden_catalog, golden_catalog_reply};
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The golden role frame `name`'s bytes.
fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The refusal of `frame` by the CatalogView.
fn refusal(frame: &[u8]) -> TestResult<ProtocolError> {
    CatalogView::from_frame(&frame_of(frame)?)
        .err()
        .context("a refusal")
}

/// The golden frame `name` with its command header changed by `change`, refused.
fn header_refusal(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolError> {
    refusal(&with_command_header(&bytes(name)?, change))
}

#[test]
fn test_the_golden_catalogs_read_their_values() -> TestResult {
    let request = bytes("catalog")?;
    let view = CatalogView::from_frame(&frame_of(&request)?)?;
    assert!(view.is_request());
    assert_eq!(
        (
            view.namespace(),
            view.version(),
            view.epoch(),
            view.position()
        ),
        (3, 0, 0, 0)
    );
    assert!(view.bytes().is_empty());
    assert_eq!(view.to_body()?, golden_catalog());
    let reply = bytes("catalog_reply")?;
    let second = CatalogView::from_frame(&frame_of(&reply)?)?;
    assert!(!second.is_request());
    assert_eq!(
        (
            second.namespace(),
            second.version(),
            second.epoch(),
            second.position()
        ),
        (3, 12, 44, 1_240)
    );
    assert_eq!(second.bytes().len(), 37);
    assert_eq!(second.to_body()?, golden_catalog_reply());
    assert_eq!(view_any(&request)?, Ok(()));
    assert_eq!(view_any(&reply)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_request_names_namespace_and_version_alone() -> TestResult {
    for at in [32, 127] {
        assert_eq!(
            header_refusal("catalog", |h| put(h, at, 1, 1))?,
            ProtocolError::reserved_not_zero(128 + at)
        );
    }
    assert_eq!(
        header_refusal("catalog", |h| put(h, 16, 5, 8))?,
        ProtocolError::field_value(518, "catalog", 5)
    );
    let with_bytes = with_section(&bytes("catalog")?, 1, 0, |s| s.push(9));
    assert_eq!(
        refusal(&with_bytes)?,
        ProtocolError::count_mismatch(518, "catalog", 1, 0)
    );
    // A request may name a version, any namespace.
    let named = with_command_header(&bytes("catalog")?, |h| {
        put(h, 0, u64::MAX, 8);
        put(h, 8, 12, 8);
    });
    assert_eq!(
        CatalogView::from_frame(&frame_of(&named)?)?.to_body()?,
        Catalog::request(u64::MAX, 12)
    );
    Ok(())
}

#[test]
fn test_a_reply_names_a_version_and_an_epoch() -> TestResult {
    assert_eq!(
        header_refusal("catalog_reply", |h| put(h, 8, 0, 8))?,
        ProtocolError::field_value(518, "version", 0)
    );
    assert_eq!(
        header_refusal("catalog_reply", |h| put(h, 16, MAX_EPOCH + 1, 8))?,
        ProtocolError::field_value(518, "epoch", MAX_EPOCH + 1)
    );
    // A pending version, applying from no epoch yet, with no bytes.
    let pending = with_section(
        &with_command_header(&bytes("catalog_reply")?, |h| put(h, 16, 0, 8)),
        1,
        0,
        Vec::clear,
    );
    let view = CatalogView::from_frame(&frame_of(&pending)?)?;
    assert_eq!(view.to_body()?, Catalog::reply(3, 12, 0, 1_240, Vec::new()));
    Ok(())
}
