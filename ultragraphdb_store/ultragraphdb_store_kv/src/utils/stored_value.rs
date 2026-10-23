/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The stored value: a version's metadata in a two-byte prefix, `[meta][flags]`, then the value. Badger keeps the
//! meta byte and the discard bit in its value struct; fjall stores bytes alone, so the port frames them.

use ultragraphdb_traits_store::Entry;

/// Badger's `bitDiscardEarlierVersions`: compaction may collect the key's older versions.
const DISCARD: u8 = 0x01;
/// A deletion, which hides the key's older versions for reads at and above it.
const TOMBSTONE: u8 = 0x02;
/// A version a drop wrote once its prefixes were empty. Compaction never collects it: a version the drop removed can
/// still reach the compaction filter, shadowed by an lsm-tree removal the filter is not shown, and must not make the
/// filter collect what the drop wrote after it.
const FENCED: u8 = 0x04;

/// A version decoded from its stored value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredValue<'a> {
    pub meta: u8,
    pub discard: bool,
    pub deletion: bool,
    pub fenced: bool,
    pub value: &'a [u8],
}

/// The stored value of `entry`, fenced when a drop writes it.
pub fn encode_value(entry: &Entry, fenced: bool) -> Vec<u8> {
    let flags = if entry.discard() { DISCARD } else { 0 }
        | if entry.is_deletion() { TOMBSTONE } else { 0 }
        | if fenced { FENCED } else { 0 };
    let value = entry.value().unwrap_or_default();
    let mut out = Vec::with_capacity(2 + value.len());
    out.push(entry.meta());
    out.push(flags);
    out.extend_from_slice(value);
    out
}

/// The version framed in `bytes`, or why it cannot be decoded.
pub fn decode_value(bytes: &[u8]) -> Result<StoredValue<'_>, String> {
    match bytes {
        [meta, flags, value @ ..] if flags & !(DISCARD | TOMBSTONE | FENCED) == 0 => {
            Ok(StoredValue {
                meta: *meta,
                discard: flags & DISCARD != 0,
                deletion: flags & TOMBSTONE != 0,
                fenced: flags & FENCED != 0,
                value,
            })
        }
        [_, flags, ..] => Err(format!("unknown version flags {flags:#04x}")),
        _ => Err("value too short to hold the version prefix".to_string()),
    }
}
