/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ColumnType;

#[test]
fn test_column_types_keep_their_numbers() {
    assert_eq!(ColumnType::Node.number(), 1);
    assert_eq!(ColumnType::Term.number(), 2);
    assert_eq!(ColumnType::from_number(1), Some(ColumnType::Node));
    assert_eq!(ColumnType::from_number(2), Some(ColumnType::Term));
    assert_eq!(ColumnType::from_number(0), None);
    assert_eq!(ColumnType::from_number(3), None);
}
