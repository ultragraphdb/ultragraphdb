/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Purge's Order (`PROTOCOL.md` §8.5) over frames laid out by hand.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Command, MAX_EPOCH, PURGE_IRIS_MAX, PurgeView};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::frame_of;
use crate::utils::term_records::{Entry, term_sections};
use crate::utils::ugs_frames::{frame, iri_terms, outcome, refused};

const A: &str = "https://example.org/deep_brain#Node/ORG%3Aacme";

fn field_value(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 24,
        field,
        value,
    }
}

/// A Purge of `epoch` and `removed` in tenant 2 naming `terms`.
fn purge(epoch: u64, removed: u32, terms: [Vec<u8>; 2]) -> TestResult<Vec<u8>> {
    frame(
        Command::Purge,
        &[
            (0, &2u64.to_le_bytes()),
            (8, &epoch.to_le_bytes()),
            (16, &removed.to_le_bytes()),
        ],
        &terms,
        true,
    )
}

/// `n` distinct IRIs.
fn iris(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("https://example.org/i/{i}"))
        .collect()
}

fn terms_of(iris: &[String]) -> [Vec<u8>; 2] {
    let refs: Vec<&str> = iris.iter().map(String::as_str).collect();
    iri_terms(&refs)
}

/// wire-protocol, "Purge and Migrate decode and are refused by stage": a well-formed Purge decodes, its tenant and
/// IRIs read in place; a reply its epoch and count.
#[test]
fn test_a_well_formed_purge_decodes() -> TestResult {
    let request = purge(0, 0, iri_terms(&[A]))?;
    let view = PurgeView::from_frame(&frame_of(&request)?)?;
    assert_eq!((view.tenant(), view.epoch(), view.removed()), (2, 0, 0));
    assert_eq!(view.iri_count(), 1);
    assert_eq!(view.iri(0), Some(A));
    assert_eq!(view.iri(1), None);
    let reply = purge(30, 1, iri_terms(&[A]))?;
    let view = PurgeView::from_frame(&frame_of(&reply)?)?;
    assert_eq!((view.epoch(), view.removed()), (30, 1));
    let body = view.to_body()?;
    assert_eq!((body.tenant(), body.epoch(), body.removed()), (2, 30, 1));
    assert_eq!(body.iris(), &[A.to_string()]);
    Ok(())
}

/// One to 10,000 IRIs: none and 10,001 refused as `CountMismatch` naming `purge`; 10,000 read.
#[test]
fn test_a_purge_names_1_to_10000_iris() -> TestResult {
    let none = purge(0, 0, [vec![], vec![]])?;
    let refusal = refused::<PurgeView>(&none)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::CountMismatch {
                command: 24,
                field: "purge",
                count: 0,
                ..
            }
        ),
        "{refusal:?}"
    );
    let at_bound = purge(0, 0, terms_of(&iris(PURGE_IRIS_MAX)))?;
    assert_eq!(outcome::<PurgeView>(&at_bound)?, None);
    let past = purge(0, 0, terms_of(&iris(PURGE_IRIS_MAX + 1)))?;
    let refusal = refused::<PurgeView>(&past)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::CountMismatch {
                command: 24,
                field: "purge",
                count: 10_001,
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

/// Each entry is an IRI, kind 1: a blank node is refused as `FieldValue` naming `purge term`, its kind 2.
#[test]
fn test_each_purged_term_is_an_iri() -> TestResult {
    let (records, text) = term_sections(&[Entry::Text(1, A, ""), Entry::Text(2, "b1", "")]);
    assert_eq!(
        refused::<PurgeView>(&purge(0, 0, [records, text])?)?,
        field_value("purge term", 2)
    );
    Ok(())
}

/// `epoch` is 0 in a request and an epoch in a reply; `removed` 0 in a request and at most the entries' count.
#[test]
fn test_epoch_and_removed_follow_request_and_reply() -> TestResult {
    let past_epoch = purge(MAX_EPOCH + 1, 1, iri_terms(&[A]))?;
    assert_eq!(
        refused::<PurgeView>(&past_epoch)?,
        field_value("epoch", MAX_EPOCH + 1)
    );
    let request_removed = purge(0, 1, iri_terms(&[A]))?;
    assert_eq!(
        refused::<PurgeView>(&request_removed)?,
        field_value("removed", 1)
    );
    let too_many = purge(30, 2, iri_terms(&[A]))?;
    assert_eq!(refused::<PurgeView>(&too_many)?, field_value("removed", 2));
    // Removing none of the IRIs named is an answer: an IRI naming none removes nothing.
    assert_eq!(outcome::<PurgeView>(&purge(30, 0, iri_terms(&[A]))?)?, None);
    assert_eq!(
        outcome::<PurgeView>(&purge(MAX_EPOCH, 1, iri_terms(&[A]))?)?,
        None
    );
    Ok(())
}

/// The command header's bytes 20 to 127 are zero.
#[test]
fn test_unused_bytes_are_zero() -> TestResult {
    let bytes = crate::utils::golden_bodies::with_command_header(
        &purge(0, 0, iri_terms(&[A]))?,
        |header| header[20] = 1,
    );
    assert_eq!(
        refused::<PurgeView>(&bytes)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 148 }
    );
    Ok(())
}

/// Every IRI is read in the order the term table holds it (§8.5), and the body holds them all.
#[test]
fn test_every_iri_is_read_in_order() -> TestResult {
    let named = iris(3);
    let bytes = purge(0, 0, terms_of(&named))?;
    let view = PurgeView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.iri_count(), 3);
    let read: Vec<Option<&str>> = (0..4).map(|index| view.iri(index)).collect();
    assert_eq!(
        read,
        vec![
            Some(named[0].as_str()),
            Some(named[1].as_str()),
            Some(named[2].as_str()),
            None
        ]
    );
    assert_eq!(view.to_body()?.iris(), named.as_slice());
    Ok(())
}
