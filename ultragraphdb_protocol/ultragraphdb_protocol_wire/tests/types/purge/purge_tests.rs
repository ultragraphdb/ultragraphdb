/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Purge's builder (`PROTOCOL.md` §8.5) against frames laid out by hand: `tenant` at 0, `epoch` at 8, `removed` at 16;
//! the instances' IRIs a request term table of kind-1 entries.

use ultragraphdb_protocol_wire::{BodyBuilder, Command, FLAG_LAST, Header, Purge};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::CLUSTER;
use crate::utils::ugs_frames::{frame, iri_terms};

const A: &str = "https://example.org/deep_brain#Node/ORG%3Aacme";
const B: &str = "https://example.org/deep_brain#Node/ORG%3Abeta";

fn header() -> Header {
    Header::new(CLUSTER, Command::Purge)
        .with_stream(3)
        .with_flags(FLAG_LAST)
}

/// A request names the tenant and the IRIs, `epoch` and `removed` 0; the reply adds the purge epoch and the count.
#[test]
fn test_a_request_and_its_reply_are_built() -> TestResult {
    let request = Purge::new(2, vec![A.to_string(), B.to_string()]);
    assert_eq!(
        (request.tenant(), request.epoch(), request.removed()),
        (2, 0, 0)
    );
    assert_eq!(request.iris(), &[A.to_string(), B.to_string()]);
    let [records, text] = iri_terms(&[A, B]);
    let expected = frame(
        Command::Purge,
        &[(0, &2u64.to_le_bytes())],
        &[records.clone(), text.clone()],
        true,
    )?;
    assert_eq!(request.frame(header())?, expected);
    let reply = request.purged(30, 1);
    assert_eq!((reply.epoch(), reply.removed()), (30, 1));
    let expected = frame(
        Command::Purge,
        &[
            (0, &2u64.to_le_bytes()),
            (8, &30u64.to_le_bytes()),
            (16, &1u32.to_le_bytes()),
        ],
        &[records, text],
        true,
    )?;
    assert_eq!(reply.frame(header())?, expected);
    Ok(())
}
