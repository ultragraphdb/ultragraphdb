/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Temporary;

#[test]
fn test_a_temporary_keyspace_is_named_by_its_number() {
    let t = Temporary::new(5);
    assert_eq!(t.id(), 5);
    assert_eq!(t, Temporary::new(5));
    assert_ne!(t, Temporary::new(6));
}
