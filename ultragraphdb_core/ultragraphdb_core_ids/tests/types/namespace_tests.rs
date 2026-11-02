/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Namespace;

#[test]
fn test_a_namespace_holds_its_value() {
    assert_eq!(Namespace::new(0).get(), 0);
    assert_eq!(Namespace::new(133).get(), 133);
    assert_eq!(Namespace::new(u64::MAX).get(), u64::MAX);
}

#[test]
fn test_distinct_namespaces_differ() {
    assert_ne!(Namespace::new(0), Namespace::new(1));
    assert!(Namespace::new(1) < Namespace::new(2));
}
