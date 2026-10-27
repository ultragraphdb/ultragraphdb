/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{RecordEntry, WriteSetRecord, WriteSetRecordView};

use crate::utils::golden_bodies::{put, section};

fn record() -> Vec<u8> {
    let mut record = WriteSetRecord::new(0, 1, 2, 3);
    record.push_dictionary(b"k1".to_vec(), Some(b"value".to_vec()));
    record.push_dictionary(b"key-two".to_vec(), None);
    record.push_dictionary(Vec::new(), Some(Vec::new()));
    record.encode()
}

#[test]
fn test_entries_read_back_in_order_with_removals() {
    let bytes = record();
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let dictionary = view.dictionary();
    assert_eq!(dictionary.len(), 3);
    assert!(!dictionary.is_empty());
    assert_eq!(
        dictionary.iter().collect::<Vec<_>>(),
        vec![
            RecordEntry::new(b"k1", Some(b"value")),
            RecordEntry::new(b"key-two", None),
            RecordEntry::new(b"", Some(b"")),
        ]
    );
    assert!(view.list_deltas().is_empty());
    assert_eq!(view.list_deltas().iter().count(), 0);
}

#[test]
fn test_entries_are_padded_to_eight_bytes() {
    let bytes = record();
    // Section 1 (dictionary) of the record, after its 32-byte fixed part: 8 + 2 + 5 = 15 → 16; 8 + 7 → 16 (removal);
    // 8 → 8: 40 bytes.
    let (offset, len) = section(&bytes, 32, 1);
    assert_eq!(len, 40);
    assert_eq!(&bytes[offset..offset + 4], &2u32.to_le_bytes());
    assert_eq!(&bytes[offset + 4..offset + 8], &5u32.to_le_bytes());
    assert_eq!(&bytes[offset + 8..offset + 15], b"k1value");
    assert_eq!(bytes[offset + 15], 0);
    assert_eq!(&bytes[offset + 20..offset + 24], &u32::MAX.to_le_bytes());
}

#[test]
fn test_an_entry_past_its_section_or_with_padding_set_is_refused() {
    let bytes = record();
    let (offset, _) = section(&bytes, 32, 1);
    let mut long = bytes.clone();
    put(&mut long, offset + 4, 1_000, 4);
    assert_eq!(
        WriteSetRecordView::new(&long).unwrap_err().0,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 17,
            section: 1,
            entry: 0,
            offset: 0
        }
    );
    let mut second = bytes.clone();
    put(&mut second, offset + 16, 1_000, 4);
    assert_eq!(
        WriteSetRecordView::new(&second).unwrap_err().0,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 17,
            section: 1,
            entry: 1,
            offset: 16
        }
    );
    let mut padded = bytes.clone();
    padded[offset + 15] = 9;
    assert_eq!(
        WriteSetRecordView::new(&padded).unwrap_err().0,
        ProtocolErrorEnum::PaddingNotZero {
            command: 17,
            offset: offset + 15
        }
    );
    // An entry header cut short: the section's last 4 bytes.
    let mut cut = bytes.clone();
    put(&mut cut, 32 + 8 + 4, 36, 4);
    assert_eq!(
        WriteSetRecordView::new(&cut).unwrap_err().0,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 17,
            section: 1,
            entry: 2,
            offset: 32
        }
    );
}
