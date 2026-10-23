/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client::utils_test::Exclusivity;

#[test]
fn only_an_exclusive_instance_may_be_destroyed() {
    assert!(Exclusivity::Exclusive.may_destroy());
    assert!(!Exclusivity::Shared.may_destroy());
}
