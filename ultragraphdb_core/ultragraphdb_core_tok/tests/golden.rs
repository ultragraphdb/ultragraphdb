/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Loading the golden vectors of tools/golden (tok.go, tok_analysis.go).

use serde::de::DeserializeOwned;

/// The cases of `testdata/golden/<name>.json`.
pub(crate) fn cases<T: DeserializeOwned + Clone>(name: &str) -> Vec<T> {
    let path = ultragraphdb_test_bdd::testdata_dir().join(format!("golden/{name}.json"));
    ultragraphdb_test_bdd::load_golden::<T>(&path)
        .cases()
        .to_vec()
}

/// The bytes of a hex string.
pub(crate) fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}
