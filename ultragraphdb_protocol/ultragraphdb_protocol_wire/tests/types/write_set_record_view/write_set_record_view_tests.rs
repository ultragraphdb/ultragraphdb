/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    OperationResult, RecordEntry, WriteSetRecord, WriteSetRecordView,
};

use crate::types::write_set_record::write_set_record_tests::{record0, record1};
use crate::utils::golden_bodies::{body_data, golden, section};

/// The golden Round's records, cut from its body by its record table.
pub fn golden_records() -> Vec<Vec<u8>> {
    let bytes = golden("round").bytes();
    let data = body_data(&bytes).to_vec();
    let (table, _) = section(&data, 0, 1);
    let (records, _) = section(&data, 0, 2);
    (0..2)
        .map(|i| {
            let offset =
                u32::from_le_bytes(data[table + 8 * i..table + 8 * i + 4].try_into().unwrap())
                    as usize;
            let len = u32::from_le_bytes(
                data[table + 8 * i + 4..table + 8 * i + 8]
                    .try_into()
                    .unwrap(),
            ) as usize;
            data[records + offset..records + offset + len].to_vec()
        })
        .collect()
}

#[test]
fn test_the_golden_records_read_their_values() {
    let records = golden_records();
    assert_eq!(record0().encode(), records[0]);
    assert_eq!(record1().encode(), records[1]);
    let view = WriteSetRecordView::new(&records[0]).unwrap();
    assert_eq!(view.bytes(), &records[0][..]);
    assert_eq!(
        (
            view.namespace(),
            view.read_version(),
            view.session(),
            view.request()
        ),
        (0, 1998, 4, 2)
    );
    assert_eq!(
        view.list_deltas().iter().collect::<Vec<_>>(),
        vec![RecordEntry::new(b"\x10key-a", Some(b"delta-a"))]
    );
    assert_eq!(
        view.dictionary().iter().collect::<Vec<_>>(),
        vec![RecordEntry::new(b"\x20dict", None)]
    );
    assert!(view.statements().is_empty());
    assert!(view.catalog().is_empty());
    assert_eq!(view.versionless().len(), 1);
    assert_eq!(view.commit_record(), b"cr");
    assert_eq!(
        view.conflict_points().iter().collect::<Vec<_>>(),
        vec![RecordEntry::new(b"\x10key-a", Some(b""))]
    );
    assert_eq!(view.conflict_statements().len(), 1);
    assert_eq!(
        view.conflict_ranges().iter().next(),
        Some(RecordEntry::new(b"\x30a", Some(b"\x30b")))
    );
    assert_eq!(view.result_count(), 2);
    assert_eq!(view.result(0), Some(OperationResult::new(0x2A, true)));
    assert_eq!(view.result(1), Some(OperationResult::new(0x2B, false)));
    assert_eq!(view.result(2), None);
    assert_eq!(view.to_record(), record0());
    let record1_view = WriteSetRecordView::new(&records[1]).unwrap();
    assert_eq!(record1_view.result_count(), 0);
    assert_eq!(record1_view.to_record(), record1());
}

#[test]
fn test_a_record_is_checked_as_a_body_with_a_fixed_part() {
    let bytes = record0().encode();
    assert_eq!(
        WriteSetRecordView::new(&bytes[..100]).unwrap_err().0,
        ProtocolErrorEnum::BodyTooShort {
            command: 17,
            len: 100,
            need: 112
        }
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        WriteSetRecordView::new(&trailing).unwrap_err().0,
        ProtocolErrorEnum::TrailingBytes {
            command: 17,
            end: 272,
            len: 273
        }
    );
    assert_eq!(
        WriteSetRecordView::new(&WriteSetRecord::default().encode())
            .unwrap()
            .to_record(),
        WriteSetRecord::default()
    );
}

#[test]
fn test_a_conflict_point_holds_no_value() {
    let mut bytes = record0().encode();
    // Section 6, the points: one entry of key_len 6, value_len 0.
    let (offset, _) = section(&bytes, 32, 6);
    bytes[offset + 4] = 1;
    // Its value byte would follow the 6-byte key inside the entry's padding: give it one so lengths stay inside.
    assert_eq!(
        WriteSetRecordView::new(&bytes).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "conflict point",
            value: 1
        }
    );
}

#[test]
fn test_results_are_whole_and_their_padding_zero() {
    let bytes = record0().encode();
    // Section 9, the results: two of 16 bytes, uid then created then 7 zero bytes.
    let (offset, len) = section(&bytes, 32, 9);
    assert_eq!(len, 32);
    let mut created = bytes.clone();
    created[offset + 8] = 2;
    assert_eq!(
        WriteSetRecordView::new(&created).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "created",
            value: 2
        }
    );
    let mut padding = bytes.clone();
    padding[offset + 16 + 12] = 1;
    assert_eq!(
        WriteSetRecordView::new(&padding).unwrap_err().0,
        ProtocolErrorEnum::PaddingNotZero {
            command: 17,
            offset: offset + 16 + 12
        }
    );
    let mut short = WriteSetRecord::new(0, 1, 2, 3).encode();
    // A results section of 8 bytes: the table's last entry names it, the bytes follow.
    let at = 32 + 8 * 9;
    let end = short.len() as u32;
    short[at..at + 4].copy_from_slice(&end.to_le_bytes());
    short[at + 4..at + 8].copy_from_slice(&8u32.to_le_bytes());
    short.extend_from_slice(&[0; 8]);
    assert_eq!(
        WriteSetRecordView::new(&short).unwrap_err().0,
        ProtocolErrorEnum::SectionLength {
            command: 17,
            section: 9,
            len: 8,
            unit: 16
        }
    );
}
