/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (ParseEdges, ConcatStrings, getInsertLayer, Uint64ToBytes,
 * BytesToUint64, isEqual, dedupeUidsPreserveOrder, worstScore, insertUidsTopKByScore), tok/index/helper.go
 * (BytesAsFloatArray) and posting/index.go (the dead list's update in addIndexMutations). Changed for ultragraphdb:
 * translated to Rust; the level draw is the caller's, where Go reads its global random source; a node is a 128-bit
 * system ID, so the entry point is its 16 big-endian bytes and the dead list a JSON array of IDs as 32 lowercase
 * hexadecimal digits, where Go's held u64 uids, and no ID stands for Go's notAUid; an entry that is no valid ID and
 * a vector whose length is not a multiple of its float width are refused, where Go panics on a short entry and keeps
 * the vector it decoded before; the scores of insertUidsTopKByScore may fail.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::similarity_type::SimilarityType;
use crate::utils::{go_math, go_sort};

/// Dgraph's `ParseEdges`: the IDs of a dead list, a JSON array whose elements are strings of 32 lowercase
/// hexadecimal digits, each a system ID's 128-bit value. Blank text and an empty array are no IDs.
///
/// # Errors
///
/// [`HnswError::malformed_dead_list`] for text that is no such array, or names no valid system ID, naming the text without
/// its outer spaces.
pub fn parse_edges(s: &str) -> Result<Vec<SystemId>, HnswError> {
    let s = s.replace(['\n', '\t'], " ");
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let refuse = || HnswError::malformed_dead_list(s);
    let inner = s
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .ok_or_else(refuse)?
        .trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|element| parse_id(element.trim()).ok_or_else(refuse))
        .collect()
}

/// The ID a dead list's element names: a JSON string of 32 lowercase hexadecimal digits, a valid system ID's value.
fn parse_id(element: &str) -> Option<SystemId> {
    let digits = element.strip_prefix('"')?.strip_suffix('"')?;
    let lowercase_hex = |b: &u8| b.is_ascii_digit() || (b'a'..=b'f').contains(b);
    if digits.len() != ID_DIGITS || !digits.as_bytes().iter().all(lowercase_hex) {
        return None;
    }
    SystemId::from_u128(u128::from_str_radix(digits, 16).ok()?).ok()
}

/// The hexadecimal digits of a system ID in a dead list.
const ID_DIGITS: usize = 32;

/// The dead list after `node` is deleted: `existing`, the dead list's value or `None` when it has none, with
/// `node`'s ID appended, as the JSON array [`parse_edges`] reads. The update `posting/index.go` makes when a vector is
/// deleted; an ID already listed is listed again, as Dgraph lists it.
///
/// # Errors
///
/// [`HnswError::malformed_dead_list`] when `existing` does not parse.
pub fn add_dead_node(existing: Option<&[u8]>, node: SystemId) -> Result<Vec<u8>, HnswError> {
    let mut dead = match existing {
        Some(value) => parse_edges(&String::from_utf8_lossy(value))?,
        None => Vec::new(),
    };
    dead.push(node);
    let listed: Vec<String> = dead
        .iter()
        .map(|id| format!("\"{:032x}\"", id.to_u128()))
        .collect();
    Ok(format!("[{}]", listed.join(",")).into_bytes())
}

/// Dgraph's `ConcatStrings`.
pub fn concat_strings(strs: &[&str]) -> String {
    strs.concat()
}

/// Dgraph's `getInsertLayer`: the level a new node is inserted from, given `rand_float`, a uniform draw in [0, 1)
/// from the caller's source. Level `max_levels - 1`, the bottom layer, holds every node; each level above holds a
/// fifth of the one below.
pub fn insert_layer(max_levels: usize, rand_float: f64) -> usize {
    (0..max_levels)
        .find(|&i| {
            let exponent = u32::try_from(max_levels - 1 - i).unwrap_or(u32::MAX);
            rand_float < go_math::pow(1.0 / 5.0, exponent)
        })
        .unwrap_or(0)
}

/// Dgraph's `BytesToUint64`: the node an entry point names, its system ID's 16 big-endian bytes; `None` unless
/// `bytes` are exactly 16 bytes of a valid ID, where Go panics on fewer and reads the first 8 of more.
#[must_use]
pub fn bytes_to_id(bytes: &[u8]) -> Option<SystemId> {
    SystemId::decode(bytes).ok()
}

/// Dgraph's `index.BytesAsFloatArray`: the little-endian floats of `encoded`; no bytes are no floats. `None` when
/// the length is not a multiple of the float width, logged at error as Dgraph logs it; Dgraph then keeps the vector
/// it decoded before.
pub fn bytes_as_float_array<T: HnswFloat>(encoded: &[u8]) -> Option<Vec<T>> {
    let width = T::BITS / 8;
    if !encoded.len().is_multiple_of(width) {
        tracing::error!(length = encoded.len(), encoded = ?encoded, "Invalid byte slice length");
        return None;
    }
    Some(encoded.chunks_exact(width).filter_map(T::from_le).collect())
}

/// Dgraph's `isEqual`.
pub fn is_equal<T: PartialEq>(a: &[T], b: &[T]) -> bool {
    a == b
}

/// Dgraph's `dedupeUidsPreserveOrder`: `uids` without repeats, in their order. No system ID stands for Go's
/// `notAUid`, so none is left out for being one.
pub(crate) fn dedupe_uids_preserve_order(uids: Vec<SystemId>) -> Vec<SystemId> {
    if uids.len() <= 1 {
        return uids;
    }
    let mut seen = std::collections::HashSet::with_capacity(uids.len());
    uids.into_iter().filter(|&uid| seen.insert(uid)).collect()
}

/// Dgraph's `worstScore`: positive infinity for a distance, negative infinity for a similarity.
pub fn worst_score<T: HnswFloat>(sim: SimilarityType) -> T {
    if sim.is_similarity_metric() {
        T::neg_infinity()
    } else {
        T::infinity()
    }
}

/// Dgraph's `insertUidsTopKByScore`: `edges`, sorted best to worst by `sim`, with `new_uids` inserted by `score`,
/// without repeats, at most `keep` long. A new ID goes after the IDs it does not beat.
///
/// # Errors
///
/// The first error of `score`.
pub fn insert_uids_top_k_by_score<T: HnswFloat, E>(
    mut edges: Vec<SystemId>,
    new_uids: &[SystemId],
    keep: usize,
    sim: SimilarityType,
    mut score: impl FnMut(SystemId) -> Result<T, E>,
) -> Result<Vec<SystemId>, E> {
    if keep == 0 {
        return Ok(Vec::new());
    }
    edges.truncate(keep);
    let mut seen = std::collections::HashSet::with_capacity(edges.len() + new_uids.len());
    edges.retain(|&uid| seen.insert(uid));
    for &uid in new_uids {
        if !seen.insert(uid) {
            continue;
        }
        let score_new = score(uid)?;
        let pos = go_sort::search(edges.len(), |i| {
            Ok(sim.is_better_score(score_new, score(edges[i])?))
        })?;
        edges.insert(pos, uid);
        edges.truncate(keep);
    }
    Ok(edges)
}
