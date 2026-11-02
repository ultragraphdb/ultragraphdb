/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `CivilTime`: the fields of Go's `time.Date`, in its order, kept as given until `DateTime::new` normalises them.

use ultragraphdb_core_types::{CivilTime, DateTime, Zone};

#[test]
fn test_civil_time_holds_go_date_fields_in_order() {
    // Go's reference time, 2006-01-02T15:04:05Z, and 7 ns.
    let t = DateTime::new(CivilTime::new(2006, 1, 2, 15, 4, 5, 7), Zone::Utc);
    assert_eq!((t.unix(), t.nanosecond()), (1_136_214_245, 7));
    assert_eq!(t, DateTime::utc(2006, 1, 2, 15, 4, 5, 7));
}

#[test]
fn test_civil_time_keeps_out_of_range_fields_as_given() {
    let thirteenth = CivilTime::new(2006, 13, 2, 15, 4, 5, 0);
    let next_january = CivilTime::new(2007, 1, 2, 15, 4, 5, 0);
    assert_ne!(thirteenth, next_january);
    assert_eq!(
        DateTime::new(thirteenth, Zone::Utc),
        DateTime::new(next_january, Zone::Utc)
    );
}
