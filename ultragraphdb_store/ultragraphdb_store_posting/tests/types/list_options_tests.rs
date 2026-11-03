/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::List as UidList;
use ultragraphdb_store_posting::ListOptions;
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::helpers::{ids, rid};

#[test]
fn test_options_build_from_their_read_timestamp() -> TestResult {
    let opt = ListOptions::at(9)
        .after(rid(3))
        .first(-2)
        .intersect(UidList::new(ids(&[4])?));
    assert_eq!(
        (opt.read_ts(), opt.after_uid(), opt.first_count()),
        (9, rid(3), -2)
    );
    assert_eq!(
        opt.intersect_with().map(|l| l.ids().to_vec()),
        Some(ids(&[4])?)
    );
    assert_eq!(ListOptions::at(1).intersect_with(), None);
    Ok(())
}
