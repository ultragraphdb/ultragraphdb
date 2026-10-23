/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::TrustMarker;

#[test]
fn test_markers_compare_by_name() {
    const A: TrustMarker = TrustMarker::new("a");
    assert_eq!(A, TrustMarker::new("a"));
    assert_ne!(A, TrustMarker::new("b"));
    assert_eq!(A.name(), "a");
}
