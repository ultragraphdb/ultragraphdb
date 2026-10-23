/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_test_bdd::compare_bytes;

#[test]
fn equal_bytes_compare_equal() {
    assert!(compare_bytes("x", &[1, 2], &[1, 2]).is_ok());
}

#[test]
fn a_mismatch_reports_both_values_in_hex() {
    let err = compare_bytes("in", &[0x0a], &[0xff]).unwrap_err();
    assert!(
        err.contains("\"in\"") && err.contains("0a") && err.contains("ff"),
        "{err}"
    );
}
