/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::PersistMode;

#[test]
fn test_the_two_modes_differ() {
    assert_ne!(PersistMode::Buffer, PersistMode::Sync);
    assert_eq!(format!("{:?}", PersistMode::Sync), "Sync");
}
