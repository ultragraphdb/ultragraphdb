/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_quads::{Page, PageEnd};

#[test]
fn test_a_next_scan_begins_just_past_a_stopped_page_and_where_a_paused_one_says() {
    assert_eq!(PageEnd::Exhausted.resume_from(), None);
    let stopped = PageEnd::Stopped { last: vec![7] };
    assert_eq!(stopped.resume_from(), Some(vec![7, 0]));
    let paused = PageEnd::Paused {
        last: None,
        from: vec![9],
        after: None,
    };
    assert_eq!(paused.resume_from(), Some(vec![9]));
    let yielded = PageEnd::Yielded { from: vec![11] };
    assert_eq!(yielded.resume_from(), Some(vec![11]));
}

#[test]
fn test_a_page_holds_its_end_and_rows() {
    let page = Page::new(PageEnd::Exhausted, 3);
    assert_eq!((page.end(), page.rows()), (&PageEnd::Exhausted, 3));
    assert_eq!(page.into_end(), PageEnd::Exhausted);
}
