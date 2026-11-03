/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

use ultragraphdb_protocol_wire::RoundHeader;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::ScannedRound;

#[test]
fn test_fields_read_back_as_given() -> TestResult {
    // Every field distinct, so a getter returning a neighbour is caught; the incarnation is the scan's, not the round's.
    let derivation = NonZeroU32::new(6).context("a derivation")?;
    let header = RoundHeader::new(3, 0xA1, 9, 40, 5, derivation);
    let round = ScannedRound::new(header, 0xB2, 8_192, 5_272);
    assert_eq!(round.position(), Position::new(3));
    assert_eq!(round.parent(), 0xA1);
    assert_eq!(round.checksum(), 0xB2);
    assert_eq!(round.first_version(), 40);
    assert_eq!(round.transactions(), 5);
    assert_eq!(round.derivation(), derivation);
    assert_eq!(round.offset(), 8_192);
    assert_eq!(round.size(), 5_272);
    Ok(())
}

#[test]
fn test_padded_size_is_whole_blocks() {
    // A frame's padded size is its size rounded up to 4,096: 256 is one block, 4,096 one, 4,097 two.
    let padded = |size| {
        let header = RoundHeader::new(1, 0, 1, 0, 0, NonZeroU32::MIN);
        ScannedRound::new(header, 0, 0, size).padded_size()
    };
    assert_eq!(padded(256), 4_096);
    assert_eq!(padded(4_095), 4_096);
    assert_eq!(padded(4_096), 4_096);
    assert_eq!(padded(4_097), 8_192);
    assert_eq!(padded(5_272), 8_192);
    assert_eq!(padded(1_048_576), 1_048_576);
}
