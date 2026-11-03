/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;

use crate::errors::quad_error::QuadError;

/// The posting of `object` in a list: an IRI or blank node by its system ID's 128 bits; a literal as a value posting
/// whose value is its encoding and whose key its 64-bit fingerprint, as `fingerprint` computes it, below every ID;
/// with the IDs of the statements holding it.
pub fn object_posting(
    object: &EncodedTerm,
    op: u32,
    statement_ids: &[SystemId],
    fingerprint: fn(&[u8]) -> u64,
) -> Posting {
    let posting = match object.as_id() {
        Some(id) => Posting::default().with_uid(id.to_u128()),
        None => Posting::default()
            .with_uid(u128::from(fingerprint(object.as_bytes())))
            .with_value(object.as_bytes().to_vec())
            .with_posting_type(PostingType::Value),
    };
    posting
        .with_op(op)
        .with_statement_ids(statement_ids.to_vec())
}

/// The object a posting of the list under `key` holds: a reference posting's key read back as a system ID, a value
/// posting's encoding; either refused as corruption naming `key` when it does not decode.
pub fn posting_object(key: &[u8], p: &Posting) -> Result<EncodedTerm, QuadError> {
    if p.posting_type() == PostingType::Ref {
        let id = SystemId::from_u128(p.uid()).map_err(|e| {
            QuadError::corrupted(key, format!("a reference posting names no system ID: {e}"))
        })?;
        return Ok(EncodedTerm::id(id));
    }
    EncodedTerm::from_bytes(p.value())
        .map_err(|e| QuadError::corrupted(key, format!("a literal posting holds no encoding: {e}")))
}
