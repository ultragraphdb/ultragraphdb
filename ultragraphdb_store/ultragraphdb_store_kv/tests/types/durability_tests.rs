/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_kv::Durability;

#[test]
fn test_the_modes_are_dgraphs_with_process_the_default() {
    assert_eq!(Durability::default(), Durability::Process);
    assert_ne!(Durability::Process, Durability::Filesystem);
}
