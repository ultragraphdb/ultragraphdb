/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a write-set record: the view never panics, and a record it accepts encodes to the same bytes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::WriteSetRecordView;

fuzz_target!(|data: &[u8]| {
    if let Ok(view) = WriteSetRecordView::new(data) {
        assert_eq!(view.to_record().encode(), data, "a record has one encoding");
        assert!(
            view.conflict_points()
                .iter()
                .all(|e| e.value() == Some(&[][..]))
        );
    }
});
