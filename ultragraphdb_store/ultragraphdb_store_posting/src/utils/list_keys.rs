/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Keyspace, QuadKey, SPLIT_FLAG, SplitStart};

use crate::errors::posting_error::PostingError;

/// Whether `key` is a list of the quad layout, split or not: its first byte is a list keyspace's.
pub fn is_quad_list(key: &[u8]) -> bool {
    key.first()
        .and_then(|b| Keyspace::from_byte(*b))
        .is_some_and(|(keyspace, _)| keyspace.is_list())
}

/// Whether `key`'s list tells its postings apart by value as well as by uid: a quad list but the vector index's,
/// whose lists hold one value each.
pub fn compares_values(key: &[u8]) -> bool {
    key.first()
        .and_then(|b| Keyspace::from_byte(*b))
        .is_some_and(|(keyspace, _)| keyspace.compares_values())
}

/// The key of the part of `key`'s list starting at the posting key `start`: the key with the split flag set and the
/// start after its kind byte, a fingerprint for a key below 2^64 and an ID for any other (`ultragraphdb_core_keys`'
/// split keys).
///
/// # Errors
///
/// [`PostingError::not_a_list`] for a key of no list keyspace, and [`PostingError::corrupted`] for a key that does
/// not parse or a start that is no valid ID, or a fingerprint on a value-index key.
pub fn split_key_of(key: &[u8], start: u128) -> Result<Vec<u8>, PostingError> {
    if !is_quad_list(key) {
        return Err(PostingError::not_a_list(key));
    }
    let start = match u64::try_from(start) {
        Ok(fingerprint) => SplitStart::Fingerprint(fingerprint),
        Err(_) => SplitStart::Id(
            SystemId::from_u128(start)
                .map_err(|_| PostingError::corrupted(key, "a split start that is no valid ID"))?,
        ),
    };
    let (list, _) = QuadKey::parse(key).map_err(|e| PostingError::corrupted(key, e.to_string()))?;
    list.split_bytes(start)
        .map_err(|e| PostingError::corrupted(key, e.to_string()))
}

/// The start of a split list's first part, the smallest key its list can hold: the smallest valid ID for a
/// value-index list, which holds references alone and splits at IDs alone, and 0, the smallest fingerprint, for any
/// other.
pub fn first_split_start(key: &[u8]) -> u128 {
    let value_index = key
        .first()
        .and_then(|b| Keyspace::from_byte(*b))
        .is_some_and(|(keyspace, _)| keyspace == Keyspace::ValueIndex);
    if value_index { SMALLEST_ID } else { 0 }
}

/// The smallest valid system ID: type prefix 1, sequence 1, bucket 0.
const SMALLEST_ID: u128 = (1 << 112) | (1 << 48);

/// Whether `key` addresses a part of a split list rather than the list itself.
///
/// # Errors
///
/// [`PostingError::not_a_list`] for a key of no list keyspace.
pub fn is_split_key(key: &[u8]) -> Result<bool, PostingError> {
    if !is_quad_list(key) {
        return Err(PostingError::not_a_list(key));
    }
    Ok(key.first().is_some_and(|b| b & SPLIT_FLAG != 0))
}

/// What tells a list's postings apart: the uid, and in a list that compares values a value posting's value as well,
/// which is the literal's encoding. Two literals whose fingerprints collide then stay two postings under one uid,
/// ordered by their encodings; in a vector-index list a uid names its one value.
pub(crate) fn identity(p: &Posting, by_value: bool) -> (u128, &[u8]) {
    if by_value && p.posting_type() != PostingType::Ref {
        (p.uid(), p.value())
    } else {
        (p.uid(), &[])
    }
}
