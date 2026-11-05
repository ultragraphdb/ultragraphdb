/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_store_posting::List;
use ultragraphdb_store_quads::{Page, PageEnd};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A page hands on the list of the key it paused inside; any other holds none.
#[test]
fn test_a_page_hands_on_the_list_it_holds() -> TestResult {
    let (end, none) = Page::new(PageEnd::Exhausted, 0).into_parts();
    assert_eq!((end, none), (PageEnd::Exhausted, None));
    let list = Arc::new(List::empty(b"a key".to_vec()));
    let page = Page::new(PageEnd::Exhausted, 1).with_held(Some(Arc::clone(&list)));
    let (_, held) = page.into_parts();
    assert!(Arc::ptr_eq(&held.context("the list")?, &list));
    Ok(())
}
