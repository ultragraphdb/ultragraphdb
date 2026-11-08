/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_terms::EncodedTerm;

use ultragraphdb_error_store::StoreError;

use crate::errors::changeset_error::ChangesetError;

/// The bytes of a key's graph, subject and predicate, before its object.
const KEY_IDS: usize = 48;

/// A statement in a logical log record (`PROTOCOL.md` §12.2, derivation version 1): its graph, subject, predicate and
/// object by their system IDs and the object's term encoding, and the statement's ID when it holds one. Its key is the
/// graph's 16 bytes (the default graph's all zero), the subject's and the predicate's 16 big-endian bytes and the
/// object's encoding; its value is the statement ID's 16 bytes, or empty for none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalQuad {
    graph: Graph,
    subject: SystemId,
    predicate: SystemId,
    object: EncodedTerm,
    statement: Option<SystemId>,
}

impl LogicalQuad {
    /// The derivation version this encoding and the keys replay derives from it belong to.
    pub const DERIVATION: u32 = 1;

    /// The statement of `object` of `subject`'s `predicate` in `graph`, with `statement`, its ID, when it holds one.
    #[must_use]
    pub fn new(
        graph: Graph,
        subject: SystemId,
        predicate: SystemId,
        object: EncodedTerm,
        statement: Option<SystemId>,
    ) -> Self {
        Self {
            graph,
            subject,
            predicate,
            object,
            statement,
        }
    }

    /// The statement's key in a record.
    #[must_use]
    pub fn key(&self) -> Vec<u8> {
        let mut key = Vec::with_capacity(KEY_IDS + self.object.as_bytes().len());
        key.extend_from_slice(&self.graph.to_be_bytes());
        key.extend_from_slice(&self.subject.to_be_bytes());
        key.extend_from_slice(&self.predicate.to_be_bytes());
        key.extend_from_slice(self.object.as_bytes());
        debug_assert!(key.len() > KEY_IDS, "a key holds its object");
        key
    }

    /// The statement's value in a record: its ID's 16 bytes, or empty.
    pub fn value(&self) -> Vec<u8> {
        self.statement
            .map_or_else(Vec::new, |id| id.to_be_bytes().to_vec())
    }

    /// The statement a record's `key` and `value` hold.
    ///
    /// # Errors
    ///
    /// The store's corruption error (`StoreError::corrupted`) naming the key for a key shorter than 48 bytes, a graph, subject or
    /// predicate that is no valid system ID (the graph's all-zero bytes aside), an object that is no term encoding, or a
    /// value neither empty nor a valid system ID.
    pub fn decode(key: &[u8], value: &[u8]) -> Result<Self, ChangesetError> {
        let corrupted = |reason: &str| -> ChangesetError {
            StoreError::corrupted(key.to_vec(), reason.to_string()).into()
        };
        let (Some((graph, rest)), true) = (key.split_at_checked(16), key.len() > KEY_IDS) else {
            return Err(corrupted(
                "a logged statement's key shorter than its IDs and object",
            ));
        };
        let (Some((subject, rest)), Some(_)) = (rest.split_at_checked(16), rest.get(32..)) else {
            return Err(corrupted(
                "a logged statement's key shorter than its IDs and object",
            ));
        };
        let (predicate, object) = rest.split_at(16);
        let id = |bytes: &[u8]| {
            SystemId::decode(bytes).map_err(|_| corrupted("a logged statement names no system ID"))
        };
        let graph = if graph.iter().all(|b| *b == 0) {
            Graph::Default
        } else {
            Graph::Named(id(graph)?)
        };
        let object = EncodedTerm::from_bytes(object)
            .map_err(|_| corrupted("a logged statement's object is no term encoding"))?;
        let statement = match value.len() {
            0 => None,
            _ => Some(id(value)?),
        };
        Ok(Self::new(
            graph,
            id(subject)?,
            id(predicate)?,
            object,
            statement,
        ))
    }

    /// The graph.
    #[must_use]
    pub fn graph(&self) -> Graph {
        self.graph
    }

    /// The subject.
    #[must_use]
    pub fn subject(&self) -> SystemId {
        self.subject
    }

    /// The predicate.
    #[must_use]
    pub fn predicate(&self) -> SystemId {
        self.predicate
    }

    /// The object's encoding.
    #[must_use]
    pub fn object(&self) -> &EncodedTerm {
        &self.object
    }

    /// The statement's ID, `None` for a statement without one.
    #[must_use]
    pub fn statement(&self) -> Option<SystemId> {
        self.statement
    }
}
