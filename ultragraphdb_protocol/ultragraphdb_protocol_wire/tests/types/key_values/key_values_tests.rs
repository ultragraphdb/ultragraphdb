/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{RecordEntry, WriteSetRecord, WriteSetRecordView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{put, section};

/// A transaction's record whose statement records, section 2, hold a value, a removal and an empty key and value.
fn record() -> Vec<u8> {
    let mut record = WriteSetRecord::transaction(0);
    record.push_statement_record(b"k1".to_vec(), Some(b"value".to_vec()));
    record.push_statement_record(b"key-two".to_vec(), None);
    record.push_statement_record(Vec::new(), Some(Vec::new()));
    record.encode()
}

/// A transaction's fixed part: kind 1, every other byte zero.
fn fixed() -> [u8; 64] {
    let mut fixed = [0u8; 64];
    fixed[0] = 1;
    fixed
}

#[test]
fn test_entries_read_back_in_order_with_removals() -> TestResult {
    let bytes = record();
    let view = WriteSetRecordView::new(&bytes)?;
    let entries = view.statement_records();
    assert_eq!(entries.len(), 3);
    assert!(!entries.is_empty());
    assert_eq!(
        entries.iter().collect::<Vec<_>>(),
        vec![
            RecordEntry::new(b"k1", Some(b"value")),
            RecordEntry::new(b"key-two", None),
            RecordEntry::new(b"", Some(b"")),
        ]
    );
    assert!(view.inserted().is_empty());
    assert_eq!(view.inserted().iter().count(), 0);
    Ok(())
}

#[test]
fn test_entries_are_padded_to_eight_bytes() {
    let bytes = record();
    // Section 2 (statement records) of the record, after its 64-byte fixed part: 8 + 2 + 5 = 15 → 16; 8 + 7 → 16
    // (removal); 8 → 8: 40 bytes.
    let (offset, len) = section(&bytes, 64, 2);
    assert_eq!(len, 40);
    assert_eq!(&bytes[offset..offset + 4], &2u32.to_le_bytes());
    assert_eq!(&bytes[offset + 4..offset + 8], &5u32.to_le_bytes());
    assert_eq!(&bytes[offset + 8..offset + 15], b"k1value");
    assert_eq!(bytes[offset + 15], 0);
    assert_eq!(&bytes[offset + 20..offset + 24], &u32::MAX.to_le_bytes());
}

#[test]
fn test_an_entry_past_its_section_or_with_padding_set_is_refused() -> TestResult {
    let bytes = record();
    let (offset, _) = section(&bytes, 64, 2);
    let mut long = bytes.clone();
    put(&mut long, offset + 4, 1_000, 4);
    assert_eq!(
        WriteSetRecordView::new(&long).err().context("a refusal")?.0,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 17,
            section: 2,
            entry: 0,
            offset: 0
        }
    );
    let mut second = bytes.clone();
    put(&mut second, offset + 16, 1_000, 4);
    assert_eq!(
        WriteSetRecordView::new(&second)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 17,
            section: 2,
            entry: 1,
            offset: 16
        }
    );
    let mut padded = bytes.clone();
    padded[offset + 15] = 9;
    assert_eq!(
        WriteSetRecordView::new(&padded)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::PaddingNotZero {
            command: 17,
            offset: offset + 15
        }
    );
    // An entry header cut short: the section's last 4 bytes.
    let mut cut = bytes.clone();
    put(&mut cut, 64 + 2 * 8 + 4, 36, 4);
    assert_eq!(
        WriteSetRecordView::new(&cut).err().context("a refusal")?.0,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 17,
            section: 2,
            entry: 2,
            offset: 32
        }
    );
    Ok(())
}

#[test]
fn test_an_entry_whose_padding_runs_past_its_section_is_refused() -> TestResult {
    use crate::utils::golden_bodies::join_sections;
    // PROTOCOL.md section 12.2: entries are back to back, each 8-aligned within the section, so an entry's padding is
    // the section's. One statement record of key "k1" and value "value", 8 + 2 + 5 = 15 bytes, without its padding
    // byte: the section ends at 15, the entry's padding at 16.
    let entry = [&2u32.to_le_bytes()[..], &5u32.to_le_bytes(), b"k1value"].concat();
    let mut secs = vec![Vec::new(); 12];
    secs[2] = entry.clone();
    assert_eq!(
        WriteSetRecordView::new(&join_sections(&fixed(), &secs))
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: 17,
            section: 2,
            entry: 0,
            offset: 0
        }
    );
    // With its padding byte it is held.
    secs[2].push(0);
    let bytes = join_sections(&fixed(), &secs);
    assert_eq!(
        WriteSetRecordView::new(&bytes)?
            .statement_records()
            .iter()
            .collect::<Vec<_>>(),
        vec![RecordEntry::new(b"k1", Some(b"value"))]
    );
    Ok(())
}
