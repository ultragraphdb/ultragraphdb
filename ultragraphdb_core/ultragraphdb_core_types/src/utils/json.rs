/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A string's JSON form, over `serde_json`.

/// The JSON string holding `s`, each byte sequence that is no UTF-8 as U+FFFD.
pub(crate) fn encode_string(s: &[u8]) -> String {
    serde_json::Value::String(String::from_utf8_lossy(s).into_owned()).to_string()
}
