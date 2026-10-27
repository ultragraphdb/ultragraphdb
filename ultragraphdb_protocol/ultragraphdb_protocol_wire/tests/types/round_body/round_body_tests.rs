/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Decision, ErrorCode, RoundBody, RoundHeader};

use crate::types::write_set_record::write_set_record_tests::{record0, record1};
use crate::utils::golden_bodies::golden;

pub fn golden_round() -> RoundBody {
    let mut round = RoundBody::new(3, 0x0F0E_0D0C_0B0A_0908, 1, 2000);
    round.commit(record0().encode());
    round.abort(ErrorCode::Conflict, 1);
    round.commit(record1().encode());
    round
}

#[test]
fn test_the_golden_round_builds_the_golden_frame() {
    let round = golden_round();
    assert_eq!(
        round.header(),
        RoundHeader::new(3, 0x0F0E_0D0C_0B0A_0908, 1, 2000, 3)
    );
    assert_eq!(
        round.decisions(),
        &[
            Decision::Committed,
            Decision::Aborted {
                code: ErrorCode::Conflict,
                detail: 1
            },
            Decision::Committed
        ]
    );
    assert_eq!(round.records(), &[record0().encode(), record1().encode()]);
    assert_eq!(round.command_header(), round.header().encode());
    assert_eq!(
        round.frame(golden("round").header()).unwrap(),
        golden("round").bytes()
    );
}

#[test]
fn test_an_empty_round_has_no_transactions() {
    let round = RoundBody::new(1, 0, 1, 2);
    assert_eq!(round.header().transactions(), 0);
    assert!(round.decisions().is_empty());
}
