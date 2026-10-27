/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::List as UidList;
use ultragraphdb_store_posting::ListOptions;

#[test]
fn test_options_build_from_their_read_timestamp() {
    let opt = ListOptions::at(9)
        .after(3)
        .first(-2)
        .intersect(UidList::new(vec![4]));
    assert_eq!(
        (opt.read_ts(), opt.after_uid(), opt.first_count()),
        (9, 3, -2)
    );
    assert_eq!(
        opt.intersect_with().map(|l| l.uids().to_vec()),
        Some(vec![4])
    );
    assert_eq!(ListOptions::at(1).intersect_with(), None);
}
