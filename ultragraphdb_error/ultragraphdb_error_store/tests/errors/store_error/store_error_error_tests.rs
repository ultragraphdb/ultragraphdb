/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_store::StoreError;

#[test]
fn test_is_a_std_error_without_a_source() {
    assert!(StoreError::Poisoned().source().is_none());
}
