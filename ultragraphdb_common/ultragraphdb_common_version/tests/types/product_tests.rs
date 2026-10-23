/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_version::PRODUCT_NAME;

#[test]
fn product_name_is_ultragraphdb() {
    assert_eq!(PRODUCT_NAME, "ultragraphdb");
}
