/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_hnsw::PersistentHeapElement;

#[test]
fn test_an_element_holds_its_score_uid_and_filter_mark() {
    let e = PersistentHeapElement::new(0.25f32, 9, true);
    assert_eq!((e.value(), e.index(), e.filtered_out()), (0.25, 9, true));
    let kept = PersistentHeapElement::new(-1.5f64, u64::MAX, false);
    assert_eq!(
        (kept.value(), kept.index(), kept.filtered_out()),
        (-1.5, u64::MAX, false)
    );
    assert_ne!(e, PersistentHeapElement::new(0.25f32, 9, false));
}
