/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A write-set record's operation, section 10: its code and argument, written and read back, and every malformed
//! operation refused.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{RecordOperation, WriteSetRecord, WriteSetRecordView};

use crate::utils::golden_bodies::section;

#[test]
fn test_each_operation_has_its_code_and_argument() {
    for (operation, code, argument) in [
        (RecordOperation::DropPredicate(7), 1, 7),
        (RecordOperation::SwapBuild(1_000), 2, 1_000),
        (RecordOperation::UndoBuild(u64::MAX), 3, u64::MAX),
    ] {
        assert_eq!((operation.code(), operation.argument()), (code, argument));
        assert_eq!(RecordOperation::from_parts(code, argument), Some(operation));
    }
    assert_eq!(RecordOperation::from_parts(0, 1), None);
    assert_eq!(RecordOperation::from_parts(4, 1), None);
}

/// A record of `operation` alone, in namespace 5.
fn operating(operation: RecordOperation) -> Vec<u8> {
    let mut record = WriteSetRecord::new(5, 0, 0, 0);
    record.set_operation(operation);
    record.encode()
}

#[test]
fn test_an_operation_is_sixteen_bytes_in_section_ten() {
    let bytes = operating(RecordOperation::SwapBuild(0x0102));
    let (offset, len) = section(&bytes, 32, 10);
    assert_eq!(len, 16);
    let mut expected = 2u16.to_le_bytes().to_vec();
    expected.extend_from_slice(&[0; 6]);
    expected.extend_from_slice(&0x0102u64.to_le_bytes());
    assert_eq!(&bytes[offset..offset + 16], expected.as_slice());
    let view = WriteSetRecordView::new(&bytes).unwrap();
    assert_eq!(view.operation(), Some(RecordOperation::SwapBuild(0x0102)));
    assert_eq!(view.to_record().encode(), bytes, "the view gives the record back");
    // A transaction's record has none.
    let plain = WriteSetRecord::new(5, 0, 0, 0).encode();
    assert_eq!(section(&plain, 32, 10).1, 0);
    assert_eq!(WriteSetRecordView::new(&plain).unwrap().operation(), None);
}

#[test]
fn test_a_malformed_operation_is_refused() {
    let bytes = operating(RecordOperation::DropPredicate(9));
    let (offset, _) = section(&bytes, 32, 10);
    let mut unknown = bytes.clone();
    unknown[offset] = 4;
    assert_eq!(
        WriteSetRecordView::new(&unknown).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "operation",
            value: 4
        }
    );
    let mut zero = bytes.clone();
    zero[offset] = 0;
    assert_eq!(
        WriteSetRecordView::new(&zero).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "operation",
            value: 0
        }
    );
    let mut padding = bytes.clone();
    padding[offset + 7] = 1;
    assert_eq!(
        WriteSetRecordView::new(&padding).unwrap_err().0,
        ProtocolErrorEnum::PaddingNotZero {
            command: 17,
            offset: offset + 7
        }
    );
    // A section of 8 bytes: the table's last entry names it, the bytes follow.
    let mut short = WriteSetRecord::new(0, 1, 2, 3).encode();
    let at = 32 + 8 * 10;
    let end = short.len() as u32;
    short[at..at + 4].copy_from_slice(&end.to_le_bytes());
    short[at + 4..at + 8].copy_from_slice(&8u32.to_le_bytes());
    short.extend_from_slice(&[0; 8]);
    assert_eq!(
        WriteSetRecordView::new(&short).unwrap_err().0,
        ProtocolErrorEnum::SectionLength {
            command: 17,
            section: 10,
            len: 8,
            unit: 16
        }
    );
}
