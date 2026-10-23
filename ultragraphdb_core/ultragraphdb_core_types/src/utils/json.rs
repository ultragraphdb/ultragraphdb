/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `encoding/json` as this crate's GeoJSON and JSON output use it, through the port of it in
//! `ultragraphdb_common_utils`: values with numbers kept as text, Go's syntax errors and Go's `Marshal` of a string.

pub(crate) use ultragraphdb_common_utils::JsonValue;
pub(crate) use ultragraphdb_common_utils::go_json::encode_string;

/// Go's `json.Unmarshal` syntax check and parse of a whole document; the error is Go's text.
pub(crate) fn parse(text: &[u8]) -> Result<JsonValue, String> {
    ultragraphdb_common_utils::go_json::unmarshal(text).map_err(|e| e.to_string())
}
