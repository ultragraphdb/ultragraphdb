/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Describe's command header (`PROTOCOL.md` §8.3): `tenant` u64 at 0, `read_version` u64 at 8, `catalog` u64 at 16,
//! nine sections. Order: unused bytes 24–127 zero; from a client, `catalog` 0 (`FieldValue` `"catalog"`); sections.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Describe, DescribeView, FrameView,
};

use crate::utils::layout::{
    CLUSTER, FRAME_MAX, built, command_header, field, frame, header, nonzero_byte, read_back,
    reserved, sections, viewed, within,
};

/// The frame of a Describe answer holding nothing, laid out by hand: its three fields, `unused` a command-header byte
/// set outside them, and nine empty sections.
fn laid_out(
    tenant: u64,
    read_version: u64,
    catalog: u64,
    unused: Option<(usize, u8)>,
) -> Option<Vec<u8>> {
    let mut fields = command_header(&[
        (0, &tenant.to_le_bytes()),
        (8, &read_version.to_le_bytes()),
        (16, &catalog.to_le_bytes()),
    ]);
    if let Some((at, byte)) = unused {
        fields[at] = byte;
    }
    frame(
        Command::Describe,
        fields,
        &sections(&vec![Vec::new(); 9]),
        0,
    )
}

/// What the client endpoint makes of `bytes`: the fields it reads, or the refusal.
fn from_client(bytes: &[u8]) -> Result<(u64, u64, u64), ProtocolErrorEnum> {
    let frame = FrameView::validate(bytes, CLUSTER, FRAME_MAX).map_err(|refusal| refusal.0)?;
    DescribeView::from_client_frame(&frame)
        .map(|view| (view.tenant(), view.read_version(), view.catalog()))
        .map_err(|refusal| refusal.0)
}

/// A Describe answer built from the drawn fields is the bytes laid out by hand, reads back to the fields, and builds
/// again to the same bytes: the catalog version among them, whatever it is.
#[hegel::test]
fn test_a_describe_answer_round_trips_its_catalog(tc: TestCase) {
    let tenant = tc.draw(gs::integers::<u64>());
    let read_version = tc.draw(gs::integers::<u64>());
    let catalog = tc.draw(gs::integers::<u64>());
    let expected = laid_out(tenant, read_version, catalog, None);
    let body = Describe::new(tenant, read_version).with_catalog(catalog);
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<DescribeView, _>(&bytes, |view| (
            view.tenant(),
            view.read_version(),
            view.catalog()
        )),
        Ok((tenant, read_version, catalog))
    );
    let again = read_back::<DescribeView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|body| built(&body).ok()), Some(bytes));
}

/// The client endpoint reads a Describe whose `catalog` is 0 and refuses any other with `FieldValue` (`"catalog"`),
/// naming it; a reply's reader takes either.
#[hegel::test]
fn test_the_client_endpoint_takes_a_catalog_of_0_alone(tc: TestCase) {
    let tenant = tc.draw(gs::integers::<u64>());
    let catalog = tc.draw(gs::integers::<u64>());
    let bytes = laid_out(tenant, 0, catalog, None).unwrap_or_default();
    let expected = if catalog == 0 {
        Ok((tenant, 0, 0))
    } else {
        Err(field(12, "catalog", catalog))
    };
    assert_eq!(from_client(&bytes), expected);
    assert_eq!(
        viewed::<DescribeView, _>(&bytes, DescribeView::catalog),
        Ok(catalog)
    );
}

/// An unused byte, 24 to 127, is refused before the catalog's rule, from a client or not.
#[hegel::test]
fn test_an_unused_byte_is_refused_before_the_catalog(tc: TestCase) {
    let at = within(&tc, 24, 127);
    let catalog = tc.draw(gs::integers::<u64>());
    let bytes = laid_out(0, 0, catalog, Some((at, nonzero_byte(&tc)))).unwrap_or_default();
    assert_eq!(from_client(&bytes), Err(reserved(at)));
    assert_eq!(viewed::<DescribeView, _>(&bytes, |_| ()), Err(reserved(at)));
}

/// The request a client builds names `catalog` 0, whatever its tenant, and the client endpoint reads it.
#[hegel::test]
fn test_a_request_names_catalog_0(tc: TestCase) {
    let tenant = tc.draw(gs::integers::<u64>());
    let request = Describe::request(tenant);
    assert_eq!(request.catalog(), 0);
    let bytes = request.frame(header(Command::Describe)).unwrap_or_default();
    assert_eq!(from_client(&bytes), Ok((tenant, 0, 0)));
}
