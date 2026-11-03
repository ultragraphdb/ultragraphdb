/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::RecordKind;

#[test]
fn test_each_kind_has_its_byte() {
    // PROTOCOL.md §12.2: 1 a transaction or a Drop, 2 a home's registration, 3 a build's start or completion.
    for (kind, code) in [
        (RecordKind::Transaction, 1u8),
        (RecordKind::Registration, 2),
        (RecordKind::Build, 3),
    ] {
        assert_eq!(kind.code(), code);
        assert_eq!(RecordKind::from_code(code), Some(kind));
    }
}

#[test]
fn test_a_byte_naming_no_kind_is_none() {
    for code in [0u8, 4, 255] {
        assert_eq!(RecordKind::from_code(code), None);
    }
}
