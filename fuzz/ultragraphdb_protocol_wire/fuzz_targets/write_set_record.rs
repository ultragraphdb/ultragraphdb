/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a logical write-set record (`PROTOCOL.md` §12.2): the view never panics, a record it accepts holds
//! what its kind takes and what its sections promise, and encodes to the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{RecordKind, WriteSetRecordView};

fuzz_target!(|data: &[u8]| {
    let Ok(view) = WriteSetRecordView::new(data) else {
        return;
    };
    assert_eq!(view.to_record().encode(), data, "a record has one encoding");
    match view.kind() {
        RecordKind::Transaction => {
            assert!(view.operation_count() <= 1);
            if let Some(drop) = view.operation(0) {
                assert!(drop.is_drop());
                assert_eq!(view.read_version(), 0);
                assert!(view.inserted().is_empty() && view.removed().is_empty());
            }
        }
        RecordKind::Registration => {
            assert_eq!(view.registration_count(), 1);
            assert_eq!(view.catalog(), 0);
        }
        RecordKind::Build => {
            assert_eq!(view.operation_count(), 1);
            assert!(view.operation(0).is_some_and(|o| !o.is_drop()));
        }
    }
    assert!(
        view.inserted()
            .iter()
            .chain(view.removed().iter())
            .all(|e| matches!(e.value().map(<[u8]>::len), Some(0 | 16)))
    );
    assert!(
        view.conflict_points()
            .iter()
            .all(|e| e.value() == Some(&[][..]))
    );
    let terms = view.result_terms().len();
    for i in 0..view.result_count() {
        let result = view.result(i).expect("a counted result reads");
        match result.node() {
            Some(node) => {
                assert!((node as usize) < terms);
                assert_eq!(result.value(), 0);
            }
            None => assert!(!result.created()),
        }
    }
});
