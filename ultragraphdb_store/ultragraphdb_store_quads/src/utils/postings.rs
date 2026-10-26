/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_proto_pb::pb::{Posting, posting};

use crate::errors::quad_error::QuadError;

/// The posting of `object` in a list: an IRI or blank node by its UID; a literal as a value posting whose value is its
/// encoding and whose uid its fingerprint, as `fingerprint` computes it; with the IDs of the statements holding it.
pub fn object_posting(
    object: &EncodedTerm,
    op: u32,
    statement_ids: &[u64],
    fingerprint: fn(&[u8]) -> u64,
) -> Posting {
    let mut posting = match object.as_uid() {
        Some(uid) => Posting {
            uid,
            ..Posting::default()
        },
        None => Posting {
            uid: fingerprint(object.as_bytes()),
            value: object.as_bytes().to_vec(),
            posting_type: posting::PostingType::Value as i32,
            ..Posting::default()
        },
    };
    posting.op = op;
    posting.statement_ids = statement_ids.to_vec();
    posting
}

/// The object a posting holds.
pub fn posting_object(key: &[u8], p: &Posting) -> Result<EncodedTerm, QuadError> {
    if p.posting_type == posting::PostingType::Ref as i32 {
        return Ok(EncodedTerm::uid(p.uid));
    }
    EncodedTerm::from_bytes(&p.value)
        .map_err(|e| QuadError::Corrupted(key, format!("a literal posting holds no encoding: {e}")))
}
