/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::EncodedTerm;

use crate::errors::quad_error::QuadError;

/// A statement's record, under its graph and statement ID: its subject, predicate and object, and its properties,
/// each a predicate's ID and a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementRecord {
    subject: SystemId,
    predicate: SystemId,
    object: EncodedTerm,
    properties: Vec<(SystemId, EncodedTerm)>,
}

/// The format of a record: a version byte, the subject's and the predicate's 16 bytes, the object's encoding, a u16
/// big-endian count of properties, each a predicate's 16 bytes and a value's encoding; encodings end where their tags
/// say.
const FORMAT: u8 = 2;

impl StatementRecord {
    /// The record of the statement `subject` `predicate` `object` with `properties`.
    pub fn new(
        subject: SystemId,
        predicate: SystemId,
        object: EncodedTerm,
        properties: Vec<(SystemId, EncodedTerm)>,
    ) -> Self {
        Self {
            subject,
            predicate,
            object,
            properties,
        }
    }

    /// The statement's subject.
    pub fn subject(&self) -> SystemId {
        self.subject
    }

    /// The statement's predicate.
    pub fn predicate(&self) -> SystemId {
        self.predicate
    }

    /// The statement's object encoding.
    pub fn object(&self) -> &EncodedTerm {
        &self.object
    }

    /// The statement's properties, each a predicate's ID and a value.
    pub fn properties(&self) -> &[(SystemId, EncodedTerm)] {
        &self.properties
    }

    /// The statement's properties, to change.
    pub fn properties_mut(&mut self) -> &mut Vec<(SystemId, EncodedTerm)> {
        &mut self.properties
    }

    /// The record in its stored format.
    pub fn encode(&self) -> Vec<u8> {
        debug_assert!(
            self.properties.len() <= usize::from(u16::MAX),
            "a record holds at most 2^16 - 1 properties"
        );
        let count = u16::try_from(self.properties.len()).unwrap_or(u16::MAX);
        let mut w = Vec::with_capacity(1 + 2 * SystemId::BYTES + self.object.as_bytes().len() + 2);
        w.push(FORMAT);
        w.extend_from_slice(&self.subject.to_be_bytes());
        w.extend_from_slice(&self.predicate.to_be_bytes());
        w.extend_from_slice(self.object.as_bytes());
        w.extend_from_slice(&count.to_be_bytes());
        for (predicate, value) in self.properties.iter().take(usize::from(count)) {
            w.extend_from_slice(&predicate.to_be_bytes());
            w.extend_from_slice(value.as_bytes());
        }
        w
    }

    /// The record stored as `bytes` under `key`, refused as corruption naming `key` when its format is unknown, it is
    /// cut short or followed by bytes, an ID is invalid or an encoding does not read.
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Self, QuadError> {
        let bad = |reason: &str| {
            QuadError::corrupted(key, format!("unreadable statement record: {reason}"))
        };
        let (format, rest) = bytes.split_first().ok_or_else(|| bad("empty"))?;
        if *format != FORMAT {
            return Err(bad("unknown format"));
        }
        let (subject, rest) = id_at(key, rest)?;
        let (predicate, rest) = id_at(key, rest)?;
        let (object, rest) = EncodedTerm::read(rest).map_err(|e| bad(&e.to_string()))?;
        let (count, mut rest) = rest
            .split_first_chunk::<2>()
            .ok_or_else(|| bad("ends early"))?;
        let count = u16::from_be_bytes(*count);
        let mut properties =
            Vec::with_capacity(usize::from(count).min(rest.len() / SystemId::BYTES));
        for _ in 0..count {
            let (p, after) = id_at(key, rest)?;
            let (value, after) = EncodedTerm::read(after).map_err(|e| bad(&e.to_string()))?;
            properties.push((p, value));
            rest = after;
        }
        if !rest.is_empty() {
            return Err(bad("bytes follow the record"));
        }
        Ok(Self {
            subject,
            predicate,
            object,
            properties,
        })
    }
}

/// The system ID `bytes` begin with, and the bytes after it: cut short or invalid, corruption naming `key`.
fn id_at<'b>(key: &[u8], bytes: &'b [u8]) -> Result<(SystemId, &'b [u8]), QuadError> {
    let (head, rest) = bytes
        .split_at_checked(SystemId::BYTES)
        .ok_or_else(|| QuadError::corrupted(key, "unreadable statement record: ends early"))?;
    let id = SystemId::decode(head)
        .map_err(|e| QuadError::corrupted(key, format!("unreadable statement record: {e}")))?;
    Ok((id, rest))
}
