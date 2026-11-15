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
        (view.tenant(), view.version(), view.epoch(), view.position()),
        (3, 0, 0, 0)
    );
    assert!(view.bytes().is_empty());
    assert_eq!(view.to_body()?, golden_catalog());
    let reply = bytes("catalog_reply")?;
    let second = CatalogView::from_frame(&frame_of(&reply)?)?;
    assert!(!second.is_request());
    assert_eq!(
        (
            second.tenant(),
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
fn test_a_request_names_tenant_and_version_alone() -> TestResult {
    for at in [32, 127] {
        assert_eq!(
            header_refusal("catalog", |h| put(h, at, 1, 1))?,
            ProtocolError::reserved_not_zero(128 + at)
        );
    }
    // A request naming a version names no epoch.
    assert_eq!(
        header_refusal("catalog", |h| {
            put(h, 8, 12, 8);
            put(h, 16, 5, 8);
        })?,
        ProtocolError::field_value(518, "catalog", 5)
    );
    let with_bytes = with_section(&bytes("catalog")?, 1, 0, |s| s.push(9));
    assert_eq!(
        refusal(&with_bytes)?,
        ProtocolError::count_mismatch(518, "catalog", 1, 0)
    );
    // A request may name a version, any tenant.
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

/// C1.100: a request of `version` 0 may name the closed epoch it asks at; one naming a `version` names none.
#[test]
fn test_a_request_names_an_epoch_only_without_a_version() -> TestResult {
    for (version, epoch) in [(0u64, 0u64), (0, 9), (3, 0)] {
        let frame = with_command_header(&bytes("catalog")?, |h| {
            put(h, 8, version, 8);
            put(h, 16, epoch, 8);
        });
        let view = CatalogView::from_frame(&frame_of(&frame)?)?;
        assert_eq!((view.version(), view.epoch()), (version, epoch));
        assert!(view.is_request());
        assert_eq!(view_any(&frame)?, Ok(()));
    }
    let at_an_epoch = with_command_header(&bytes("catalog")?, |h| put(h, 16, 9, 8));
    assert_eq!(
        CatalogView::from_frame(&frame_of(&at_an_epoch)?)?.to_body()?,
        Catalog::request_at(3, 9)
    );
    assert_eq!(
        header_refusal("catalog", |h| {
            put(h, 8, 3, 8);
            put(h, 16, 9, 8);
        })?,
        ProtocolError::field_value(518, "catalog", 9)
    );
    // The header's rules come before the sections: a request of `version` 0 at epoch 9 holding a byte.
    let with_bytes = with_section(&at_an_epoch, 1, 0, |s| s.push(9));
    assert_eq!(
        refusal(&with_bytes)?,
        ProtocolError::count_mismatch(518, "catalog", 1, 0)
    );
    Ok(())
}

/// C1.101: a request's `epoch` is an epoch field (`PROTOCOL.md` §1, §13.7): at most 2⁴² − 1, checked after the rule of
/// a request naming a `version`.
#[test]
fn test_a_requests_epoch_is_within_42_bits() -> TestResult {
    let most = with_command_header(&bytes("catalog")?, |h| put(h, 16, MAX_EPOCH, 8));
    assert_eq!(
        CatalogView::from_frame(&frame_of(&most)?)?.epoch(),
        MAX_EPOCH
    );
    for epoch in [MAX_EPOCH + 1, u64::MAX] {
        assert_eq!(
            header_refusal("catalog", |h| put(h, 16, epoch, 8))?,
            ProtocolError::field_value(518, "epoch", epoch)
        );
    }
    assert_eq!(
        header_refusal("catalog", |h| {
            put(h, 8, 3, 8);
            put(h, 16, MAX_EPOCH + 1, 8);
        })?,
        ProtocolError::field_value(518, "catalog", MAX_EPOCH + 1)
    );
    Ok(())
}
