/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::BindKind;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_each_kind_has_its_byte_and_no_other_byte_names_one() -> TestResult {
    assert_eq!(BindKind::Transaction.number(), 1);
    assert_eq!(BindKind::Drop.number(), 2);
    for kind in [BindKind::Transaction, BindKind::Drop] {
        assert_eq!(BindKind::from_number(kind.number()), Some(kind));
    }
    for byte in [0u8, 3, 255] {
        assert_eq!(BindKind::from_number(byte), None);
    }
    Ok(())
}
