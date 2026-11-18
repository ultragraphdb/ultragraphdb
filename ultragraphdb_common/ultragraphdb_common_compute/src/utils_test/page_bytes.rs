/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The bytes a read's page holds of its rows' object encodings once it has taken the object records `value` under the list
/// key `key`, every record and statement ID taken: one frame's page holds each encoding once, so its bytes are bounded by
/// its frame's (design D3). `None` for a key or a value the page does not read.
#[must_use]
pub fn page_held_bytes(key: &[u8], value: &[u8]) -> Option<usize> {
    crate::types::partition_rows::page_bytes_of(key, value)
}
