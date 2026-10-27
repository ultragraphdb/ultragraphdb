/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, ColumnType, PatternSlot, Prepare, QuadPattern};

use crate::utils::golden_bodies::golden;

pub fn pattern() -> QuadPattern<String> {
    QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound("<http://ex.org/name>".to_string()),
        PatternSlot::Unbound,
    )
}

#[test]
fn test_prepare_and_its_reply_build_the_golden_frames() {
    let request = Prepare::new(0, 0, 0, pattern(), vec![ColumnType::Node], Vec::new());
    assert_eq!(
        request.frame(golden("prepare").header()).unwrap(),
        golden("prepare").bytes()
    );
    let reply = Prepare::new(
        0,
        1,
        0x5C4E_3A2B_1D0F_7E6A,
        pattern(),
        vec![ColumnType::Node],
        vec![ColumnType::Term],
    );
    assert_eq!(
        reply.frame(golden("prepare_reply").header()).unwrap(),
        golden("prepare_reply").bytes()
    );
    assert_eq!(
        (reply.namespace(), reply.statement(), reply.schema()),
        (0, 1, 0x5C4E_3A2B_1D0F_7E6A)
    );
    assert_eq!(reply.pattern(), &pattern());
    assert_eq!(reply.parameter_types(), &[ColumnType::Node]);
    assert_eq!(reply.result_types(), &[ColumnType::Term]);
}
