/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use ultragraphdb_core_codec::{Posting, PostingType};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_store_posting::{List, PostingError, PostingIdentity};

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

/// The statement IDs `list` holds `object` under at its own read timestamp, `None` when it holds no posting of it: a seek
/// to the uid [`object_posting`] gives the object, reading no other object's posting.
pub(crate) fn held_object(
    list: &List,
    object: &EncodedTerm,
    fingerprint: fn(&[u8]) -> u64,
) -> Result<Option<Vec<SystemId>>, PostingError> {
    let read_ts = list.mutation().read_ts();
    let found = match object.as_id() {
        Some(id) => list.find_posting_matching(read_ts, id.to_u128(), |p| {
            p.posting_type() == PostingType::Ref
        })?,
        None => {
            let uid = u128::from(fingerprint(object.as_bytes()));
            list.find_posting_matching(read_ts, uid, |p| {
                p.posting_type() == PostingType::Value && p.value() == object.as_bytes()
            })?
        }
    };
    Ok(found.map(|p| p.statement_ids().to_vec()))
}

/// Where a list of `keyspace` resumes for a reader naming the object `encoding` (`PROTOCOL.md` §13.13): at the identity
/// [`object_posting`] gives that object, read itself when the list holds it, a literal's value part of it only where the
/// keyspace compares values.
///
/// # Errors
///
/// [`TermError`] for bytes that are not one whole object encoding.
pub fn object_identity(
    keyspace: Keyspace,
    encoding: &[u8],
    fingerprint: fn(&[u8]) -> u64,
) -> Result<PostingIdentity, TermError> {
    EncodedTerm::check(encoding)?;
    let object = EncodedTerm::from_bytes(encoding)?;
    let identity = match object.as_id() {
        Some(id) => PostingIdentity::new(id.to_u128(), Vec::new()),
        None if keyspace.compares_values() => {
            PostingIdentity::new(u128::from(fingerprint(encoding)), encoding.to_vec())
        }
        None => PostingIdentity::new(u128::from(fingerprint(encoding)), Vec::new()),
    };
    Ok(identity.included())
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

/// Whether `list` holds a posting of `id` at `read_ts`, as a predicate list names a predicate: the list's iteration
/// seeks the ID and reads the one posting there, allocating nothing.
pub(crate) fn list_holds(list: &List, read_ts: u64, id: SystemId) -> Result<bool, PostingError> {
    let uid = id.to_u128();
    let mut held = false;
    // A system ID is never 0, since its type prefix is not, so the uid before it is its own less one.
    list.iterate(read_ts, uid.saturating_sub(1), |p| {
        held = p.uid() == uid;
        Ok::<_, PostingError>(ControlFlow::Break(()))
    })?;
    Ok(held)
}
