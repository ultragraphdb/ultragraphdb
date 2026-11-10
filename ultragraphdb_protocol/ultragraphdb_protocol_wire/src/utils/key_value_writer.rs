/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A key-value section: entries of `key_len` u32, `value_len` u32 (`u32::MAX` for a removal, which holds no value),
//! the key, the value, then zeros to a multiple of 8.

use ultragraphdb_core_body::{align8, as_u32};

use crate::utils::constants::ABSENT;

/// The bytes an entry of a key of `key_len` and a value of `value_len` takes in its section, padding included.
pub(crate) fn entry_size(key_len: usize, value_len: usize) -> usize {
    align8(8usize.saturating_add(key_len).saturating_add(value_len))
}

/// The section of `entries`, each a key and its value, `None` for a removal.
pub(crate) fn write_entries<'e>(
    entries: impl IntoIterator<Item = (&'e [u8], Option<&'e [u8]>)>,
) -> Vec<u8> {
    let mut out = Vec::new();
    for (key, value) in entries {
        out.extend_from_slice(&as_u32(key.len()).to_le_bytes());
        out.extend_from_slice(&value.map_or(ABSENT, |v| as_u32(v.len())).to_le_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(value.unwrap_or_default());
        out.resize(align8(out.len()), 0);
    }
    out
}
