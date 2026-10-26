/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{Keyspace, SPLIT_FLAG, parse, split_key};
use ultragraphdb_proto_pb::pb::{Posting, posting};

use crate::errors::posting_error::PostingError;

/// Whether `key` is a list of the quad layout, split or not: its first byte is a list keyspace's. Dgraph's keys
/// begin with a prefix byte of 0x00 or 0x04 and never collide with one.
pub fn is_quad_list(key: &[u8]) -> bool {
    key.first()
        .and_then(|b| Keyspace::from_byte(*b))
        .is_some_and(|(keyspace, _)| keyspace.is_list())
}

/// The key of the part of `key`'s list starting at `start`, in the key's own layout.
pub fn split_key_of(key: &[u8], start: u64) -> Result<Vec<u8>, PostingError> {
    if is_quad_list(key) {
        let mut part = key.to_vec();
        part[0] |= SPLIT_FLAG;
        part.extend_from_slice(&start.to_be_bytes());
        return Ok(part);
    }
    Ok(split_key(key, start)?)
}

/// Whether `key` addresses a part of a split list rather than the list itself.
pub fn is_split_key(key: &[u8]) -> Result<bool, PostingError> {
    if is_quad_list(key) {
        return Ok(key[0] & SPLIT_FLAG != 0);
    }
    Ok(parse(key)?.has_start_uid())
}

/// What tells a list's postings apart: the uid, and in a quad list a value posting's value as well, which is the
/// literal's encoding. Two literals whose fingerprints collide then stay two postings under one uid, ordered by
/// their encodings; in a list of Dgraph's layout a uid names one posting, as a scalar predicate's overwrite needs.
pub(crate) fn identity(p: &Posting, by_value: bool) -> (u64, &[u8]) {
    if by_value && p.posting_type != posting::PostingType::Ref as i32 {
        (p.uid, &p.value)
    } else {
        (p.uid, &[])
    }
}
