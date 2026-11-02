/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_sequencer::CatalogVersion;
use ultragraphdb_core_ids::Namespace;

#[test]
fn test_a_version_keeps_its_namespace_and_number() {
    let version = CatalogVersion::new(Namespace::new(9), 3);
    assert_eq!(version.namespace(), Namespace::new(9));
    assert_eq!(version.version(), 3);
    let largest = CatalogVersion::new(Namespace::new(u64::MAX), u64::MAX);
    assert_eq!(
        (largest.namespace().get(), largest.version()),
        (u64::MAX, u64::MAX)
    );
}

#[test]
fn test_versions_order_by_namespace_then_number() {
    let a = CatalogVersion::new(Namespace::new(1), 9);
    let b = CatalogVersion::new(Namespace::new(2), 1);
    let c = CatalogVersion::new(Namespace::new(2), 2);
    assert!(a < b);
    assert!(b < c);
}
