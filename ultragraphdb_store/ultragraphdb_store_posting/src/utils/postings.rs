/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (NewPosting, createDeleteAllPosting, hasDeleteAll, fingerprintEdge,
 * getLengthDelta, isPlistEmpty). Changed for ultragraphdb: translated to Rust; an edge's unknown operation is an
 * error where Dgraph exits the process, and whether the predicate is a list is the caller's, where Dgraph asks its
 * global schema.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_farm::fingerprint64;
use ultragraphdb_core_keys::STAR;
use ultragraphdb_proto_pb::pb::{DirectedEdge, Posting, PostingList, directed_edge, posting};

use crate::errors::posting_error::PostingError;
use crate::utils::constants::{DEL, OVR, SET};

/// The posting an edge writes: Dgraph's `NewPosting`. Its uid is the edge's target; a value's uid is set by the
/// caller from [`fingerprint_edge`].
///
/// # Errors
///
/// [`PostingError::UnhandledOperation`] for an operation that is none of set, delete and overwrite; Dgraph exits
/// the process with `x.Fatalf`.
pub fn new_posting(edge: &DirectedEdge) -> Result<Posting, PostingError> {
    let op = match directed_edge::Op::try_from(edge.op) {
        Ok(directed_edge::Op::Set) => SET,
        Ok(directed_edge::Op::Ovr) => OVR,
        Ok(directed_edge::Op::Del) => DEL,
        Err(_) => return Err(PostingError::UnhandledOperation(edge.op)),
    };
    let posting_type = if !edge.lang.is_empty() {
        posting::PostingType::ValueLang
    } else if edge.value_id == 0 {
        posting::PostingType::Value
    } else {
        posting::PostingType::Ref
    };
    Ok(Posting {
        uid: edge.value_id,
        value: edge.value.clone(),
        val_type: edge.value_type,
        posting_type: posting_type as i32,
        lang_tag: edge.lang.as_bytes().to_vec(),
        op,
        facets: edge.facets.clone(),
        ..Posting::default()
    })
}

/// The posting of a delete-all: a delete of the value `_STAR_ALL` at uid `u64::MAX`.
pub fn delete_all_posting() -> Posting {
    Posting {
        op: DEL,
        value: STAR.as_bytes().to_vec(),
        uid: u64::MAX,
        ..Posting::default()
    }
}

/// Whether `posting` is a delete-all: a delete of `_STAR_ALL` without a language.
pub fn has_delete_all(posting: &Posting) -> bool {
    posting.op == DEL && posting.value == STAR.as_bytes() && posting.lang_tag.is_empty()
}

/// The uid identifying a value posting: the fingerprint of its language when it has one, of its value for a list
/// predicate, and `u64::MAX` otherwise, so that a subject holds one untagged value of a scalar predicate.
pub fn fingerprint_edge(edge: &DirectedEdge, is_list: bool) -> u64 {
    if !edge.lang.is_empty() {
        fingerprint64(edge.lang.as_bytes())
    } else if is_list {
        fingerprint64(&edge.value)
    } else {
        u64::MAX
    }
}

/// What a posting's operation adds to a list's length.
pub fn length_delta(op: u32) -> i64 {
    match op {
        DEL => -1,
        SET => 1,
        _ => 0,
    }
}

/// Whether `plist` holds nothing: no splits and no uids.
pub fn is_plist_empty(plist: &PostingList) -> bool {
    plist.splits.is_empty() && plist.pack.as_ref().is_none_or(|p| p.blocks.is_empty())
}
