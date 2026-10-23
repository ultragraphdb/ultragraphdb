/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (ParseEdges, ConcatStrings, getInsertLayer, Uint64ToBytes,
 * BytesToUint64, isEqual, dedupeUidsPreserveOrder, worstScore, insertUidsTopKByScore), tok/index/helper.go
 * (BytesAsFloatArray) and posting/index.go (the dead list's update in addIndexMutations). Changed for ultragraphdb:
 * translated to Rust; the level draw is the caller's, where Go reads its global random source; an entry of other
 * than 8 bytes and a vector whose length is not a multiple of its float width are refused, where Go panics on a
 * short entry and keeps the vector it decoded before; the scores of insertUidsTopKByScore may fail.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_strconv;

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::similarity_type::SimilarityType;
use crate::utils::{go_math, go_sort};

/// The uid that is no uid, Dgraph's `notAUid`.
pub const NOT_A_UID: u64 = 0;

/// Dgraph's `ParseEdges`: the uids of a dead list, a JSON array of uids, or uids in brackets separated by spaces.
/// Blank text is no uids.
///
/// # Errors
///
/// [`HnswError::UintSlice`] for text that is neither, naming the text without its outer spaces.
pub fn parse_edges(s: &str) -> Result<Vec<u64>, HnswError> {
    let s = s.replace(['\n', '\t'], " ");
    let s = s.trim();
    if s.is_empty() {
        return Ok(Vec::new());
    }
    let refuse = || HnswError::UintSlice(s);
    let trimmed_pre = s.strip_prefix('[').ok_or_else(refuse)?;
    let trimmed = trimmed_pre.trim_end_matches(']');
    if trimmed.len() == trimmed_pre.len() {
        return Err(refuse());
    }
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let parse = |v: &str| go_strconv::parse_uint(v, 10).map_err(|_| refuse());
    if trimmed.contains(',') {
        return trimmed.split(',').map(|v| parse(v.trim())).collect();
    }
    trimmed
        .split(' ')
        .filter(|v| !v.is_empty())
        .map(parse)
        .collect()
}

/// The dead list after `uid` is deleted: `existing`, the dead list's value or `None` when it has none, with `uid`
/// appended, as JSON, Go's `json.Marshal` of a `[]uint64`. The update `posting/index.go` makes when a vector is
/// deleted; a uid already listed is listed again, as Dgraph lists it.
///
/// # Errors
///
/// [`HnswError::UintSlice`] when `existing` does not parse.
pub fn add_dead_node(existing: Option<&[u8]>, uid: u64) -> Result<Vec<u8>, HnswError> {
    let mut dead = match existing {
        Some(value) => parse_edges(&String::from_utf8_lossy(value))?,
        None => Vec::new(),
    };
    dead.push(uid);
    let listed: Vec<String> = dead.iter().map(u64::to_string).collect();
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

/// Dgraph's `Uint64ToBytes`: big-endian.
pub fn uint64_to_bytes(key: u64) -> Vec<u8> {
    key.to_be_bytes().to_vec()
}

/// Dgraph's `BytesToUint64`: the big-endian uid of an entry; `None` unless `bytes` hold exactly 8 bytes, where Go
/// panics on fewer and reads the first 8 of more.
pub fn bytes_to_uint64(bytes: &[u8]) -> Option<u64> {
    bytes.try_into().ok().map(u64::from_be_bytes)
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

/// Dgraph's `dedupeUidsPreserveOrder`: `uids` without `notAUid` and without repeats, in their order.
pub(crate) fn dedupe_uids_preserve_order(uids: Vec<u64>) -> Vec<u64> {
    if uids.len() <= 1 {
        return uids;
    }
    let mut seen = std::collections::HashSet::with_capacity(uids.len());
    uids.into_iter()
        .filter(|&uid| uid != NOT_A_UID && seen.insert(uid))
        .collect()
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
/// without `notAUid` and repeats, at most `keep` long. A new uid goes after the uids it does not beat.
///
/// # Errors
///
/// The first error of `score`.
pub fn insert_uids_top_k_by_score<T: HnswFloat, E>(
    mut edges: Vec<u64>,
    new_uids: &[u64],
    keep: usize,
    sim: SimilarityType,
    mut score: impl FnMut(u64) -> Result<T, E>,
) -> Result<Vec<u64>, E> {
    if keep == 0 {
        return Ok(Vec::new());
    }
    edges.truncate(keep);
    let mut seen = std::collections::HashSet::with_capacity(edges.len() + new_uids.len());
    edges.retain(|&uid| uid != NOT_A_UID && seen.insert(uid));
    for &uid in new_uids {
        if uid == NOT_A_UID || !seen.insert(uid) {
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
