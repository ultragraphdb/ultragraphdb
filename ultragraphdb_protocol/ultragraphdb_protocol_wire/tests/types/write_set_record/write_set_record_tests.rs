/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{OperationResult, WriteSetRecord, WriteSetRecordView};

/// Record 0 of the golden Round, as generate_bodies.py lays it out.
pub fn record0() -> WriteSetRecord {
    let mut record = WriteSetRecord::new(0, 1998, 4, 2);
    record.push_list_delta(b"\x10key-a".to_vec(), b"delta-a".to_vec());
    record.push_dictionary(b"\x20dict".to_vec(), None);
    record.push_versionless(b"sess-4".to_vec(), Some(b"reply".to_vec()));
    record.set_commit_record(b"cr".to_vec());
    record.push_conflict_point(b"\x10key-a".to_vec());
    record.push_conflict_statement(b"\x10key-a".to_vec(), b"\x01obj".to_vec());
    record.push_conflict_range(b"\x30a".to_vec(), b"\x30b".to_vec());
    record.push_result(OperationResult::new(0x2A, true));
    record.push_result(OperationResult::new(0x2B, false));
    record
}

/// Record 1: a 5,000-byte delta.
pub fn record1() -> WriteSetRecord {
    let mut record = WriteSetRecord::new(0, 1990, 6, 9);
    record.push_list_delta(
        b"\x10big".to_vec(),
        (0..5_000u32).map(|i| ((i * 31 + 7) % 251) as u8).collect(),
    );
    record
}

#[test]
fn test_a_record_keeps_its_fields_and_tables() {
    let record = record0();
    assert_eq!(
        (
            record.namespace(),
            record.read_version(),
            record.session(),
            record.request()
        ),
        (0, 1998, 4, 2)
    );
    assert_eq!(
        record.list_deltas(),
        &[(b"\x10key-a".to_vec(), Some(b"delta-a".to_vec()))]
    );
    assert_eq!(record.dictionary(), &[(b"\x20dict".to_vec(), None)]);
    assert!(record.statements().is_empty());
    assert!(record.catalog().is_empty());
    assert_eq!(
        record.versionless(),
        &[(b"sess-4".to_vec(), Some(b"reply".to_vec()))]
    );
    assert_eq!(record.commit_record(), b"cr");
    assert_eq!(record.conflict_points(), &[b"\x10key-a".to_vec()]);
    assert_eq!(
        record.conflict_statements(),
        &[(b"\x10key-a".to_vec(), b"\x01obj".to_vec())]
    );
    assert_eq!(
        record.conflict_ranges(),
        &[(b"\x30a".to_vec(), b"\x30b".to_vec())]
    );
    assert_eq!(
        record.results(),
        &[
            OperationResult::new(0x2A, true),
            OperationResult::new(0x2B, false)
        ]
    );
    assert!(record1().results().is_empty());
    // generate_bodies.py printed 272 and 5,128 bytes.
    assert_eq!(record0().encode().len(), 272);
    assert_eq!(record1().encode().len(), 5_128);
    let mut all = WriteSetRecord::new(1, 2, 3, 4);
    all.push_statement(b"s".to_vec(), Some(b"v".to_vec()));
    all.push_catalog(b"c".to_vec(), None);
    let bytes = all.encode();
    assert_eq!(WriteSetRecordView::new(&bytes).unwrap().to_record(), all);
}
