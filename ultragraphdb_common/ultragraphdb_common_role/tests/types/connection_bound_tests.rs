/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::ConnectionBound;

/// A bound of two counts each connection while it is held: the first two lie within it, the third past it, and each that ends
/// leaves the count.
#[test]
fn test_a_bound_counts_what_it_holds() {
    let bound = ConnectionBound::new(2);
    assert_eq!(bound.held(), 0);
    let first = bound.enter();
    let second = bound.enter();
    let third = bound.enter();
    assert_eq!(bound.held(), 3);
    assert_eq!(
        (first.within(), second.within(), third.within()),
        (true, true, false)
    );
    assert_eq!(third.limit(), 2);
    drop(third);
    assert_eq!(bound.held(), 2);
    let again = bound.enter();
    assert!(!again.within(), "two are still held");
    drop((first, second, again));
    assert_eq!(bound.held(), 0);
    assert!(bound.enter().within());
}
