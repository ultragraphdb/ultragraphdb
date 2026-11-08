/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::Stopping;

/// A role is not stopping until the service says so, and then every clone of it is: the role's cores share one flag.
#[test]
fn test_a_stop_is_seen_by_every_clone() {
    let stopping = Stopping::default();
    let clone = stopping.clone();
    assert!(!stopping.is_stopping() && !clone.is_stopping());
    stopping.set();
    assert!(stopping.is_stopping() && clone.is_stopping());
}
