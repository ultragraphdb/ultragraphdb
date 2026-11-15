/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Catalog request naming an epoch (`PROTOCOL.md` §13.7): `tenant` at 0, `version` at 8, `epoch` at 16, `position`
//! 0 at 24, section 0 empty. Order: unused bytes 32–127 zero; in a request naming a `version`, `epoch` 0; in a request
//! of `version` 0, `epoch` at most 2⁴² − 1; sections; section 0 empty.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_protocol_wire::{Catalog, CatalogView, Command};

use crate::utils::layout::{
    EPOCH_MAX, built, command_header, count, field, frame, outcome, read_back, sections, viewed,
    within,
};

/// The Catalog request laid out by hand, its section 0 `bytes`.
fn laid_out(tenant: u64, version: u64, epoch: u64, bytes: Vec<u8>) -> Option<Vec<u8>> {
    let header = command_header(&[
        (0, &tenant.to_le_bytes()),
        (8, &version.to_le_bytes()),
        (16, &epoch.to_le_bytes()),
    ]);
    frame(Command::Catalog, header, &sections(&[bytes]), 0)
}

/// L1.13: a Catalog request of `version` 0 at a drawn epoch, or of a drawn `version` at none, is the bytes laid out
/// by hand, reads back to its fields and builds again to the same bytes.
#[hegel::test]
fn test_a_catalog_request_round_trips(tc: TestCase) {
    let tenant = tc.draw(gs::integers::<u64>());
    let (version, epoch) = if tc.draw(gs::booleans()) {
        (0, tc.draw(gs::integers::<u64>().max_value(EPOCH_MAX)))
    } else {
        (tc.draw(gs::integers::<u64>().min_value(1)), 0)
    };
    let expected = laid_out(tenant, version, epoch, Vec::new());
    let body = if version == 0 {
        Catalog::request_at(tenant, epoch)
    } else {
        Catalog::request(tenant, version)
    };
    assert_eq!(
        (body.tenant(), body.version(), body.epoch(), body.position()),
        (tenant, version, epoch, 0)
    );
    assert_eq!(Catalog::request_at(tenant, 0), Catalog::request(tenant, 0));
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<CatalogView, _>(&bytes, |view| {
            (
                view.tenant(),
                view.version(),
                view.epoch(),
                view.is_request(),
            )
        }),
        Ok((tenant, version, epoch, true))
    );
    let again = read_back::<CatalogView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|body| built(&body).ok()), Some(bytes));
}

/// L1.21, Catalog: a request naming a `version` and an epoch, one of `version` 0 whose epoch is past 2⁴² − 1 (case
/// C1.101), and one whose section 0 is not empty, is refused with its rule's refusal, the header's before the
/// section's.
#[hegel::test]
fn test_a_catalog_request_breaking_a_rule_is_refused_with_that_rule(tc: TestCase) {
    let version = tc.draw(gs::integers::<u64>().min_value(1));
    let epoch = tc.draw(gs::integers::<u64>().min_value(1).max_value(EPOCH_MAX));
    let past = tc.draw(gs::integers::<u64>().min_value(EPOCH_MAX + 1));
    let bytes = vec![7u8; within(&tc, 1, 40)];
    let held = bytes.len() as u64;
    let (laid, refusal) = match within(&tc, 0, 4) {
        0 => (
            laid_out(3, version, epoch, Vec::new()),
            field(518, "catalog", epoch),
        ),
        1 => (laid_out(3, 0, past, Vec::new()), field(518, "epoch", past)),
        2 => (laid_out(3, 0, epoch, bytes), count(518, "catalog", held, 0)),
        3 => (
            laid_out(3, version, past, bytes),
            field(518, "catalog", past),
        ),
        _ => (laid_out(3, 0, past, bytes), field(518, "epoch", past)),
    };
    assert!(laid.is_some());
    assert_eq!(
        outcome::<CatalogView>(&laid.unwrap_or_default()),
        Err(refusal)
    );
}
