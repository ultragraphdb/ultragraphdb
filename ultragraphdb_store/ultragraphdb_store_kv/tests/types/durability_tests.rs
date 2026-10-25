/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_kv::Durability;

#[test]
fn test_the_default_mode_is_dgraphs_default_process() {
    // Dgraph's `--survive` defaults to `process` (`x/flags.go:41-47`).
    assert_eq!(Durability::default(), Durability::Process);
}
