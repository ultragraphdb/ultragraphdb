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
    assert_eq!(ColumnType::from_number(14), None);
    let numbers: Vec<u8> = ColumnType::ALL.iter().map(|t| t.number()).collect();
    assert_eq!(numbers, (1..=13).collect::<Vec<u8>>());
    for column in ColumnType::ALL {
        assert_eq!(ColumnType::from_number(column.number()), Some(column));
        assert_eq!(column.in_pattern(), column.number() <= 2, "{column:?}");
    }
}
