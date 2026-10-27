/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_core_keys::{Keyspace, SPLIT_FLAG};

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

/// The key of the part of `key`'s list starting at `start`: the key with the split flag set and the start appended.
///
/// # Errors
///
/// [`PostingError::NotAList`] for a key of no list keyspace.
pub fn split_key_of(key: &[u8], start: u64) -> Result<Vec<u8>, PostingError> {
    if !is_quad_list(key) {
        return Err(PostingError::NotAList(key));
    }
    let mut part = key.to_vec();
    part[0] |= SPLIT_FLAG;
    part.extend_from_slice(&start.to_be_bytes());
    Ok(part)
}

/// Whether `key` addresses a part of a split list rather than the list itself.
///
/// # Errors
///
/// [`PostingError::NotAList`] for a key of no list keyspace.
pub fn is_split_key(key: &[u8]) -> Result<bool, PostingError> {
    if !is_quad_list(key) {
        return Err(PostingError::NotAList(key));
    }
    Ok(key[0] & SPLIT_FLAG != 0)
}

/// What tells a list's postings apart: the uid, and in a list that compares values a value posting's value as well,
/// which is the literal's encoding. Two literals whose fingerprints collide then stay two postings under one uid,
/// ordered by their encodings; in a vector-index list a uid names its one value.
pub(crate) fn identity(p: &Posting, by_value: bool) -> (u64, &[u8]) {
    if by_value && p.posting_type() != PostingType::Ref {
        (p.uid(), p.value())
    } else {
        (p.uid(), &[])
    }
}
