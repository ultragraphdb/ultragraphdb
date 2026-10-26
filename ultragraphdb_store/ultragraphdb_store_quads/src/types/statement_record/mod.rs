/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;

use crate::errors::quad_error::QuadError;

/// A statement's record, under its graph and statement ID: its subject, predicate and object, and its properties,
/// each a predicate's UID and a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementRecord {
    subject: u64,
    predicate: u64,
    object: EncodedTerm,
    properties: Vec<(u64, EncodedTerm)>,
}

/// The format of a record: a version byte, the subject and predicate as u64 big-endian, the object's encoding, a u16
/// count of properties, each a predicate's u64 and a value's encoding; encodings end where their tags say.
const FORMAT: u8 = 1;

impl StatementRecord {
    pub fn new(
        subject: u64,
        predicate: u64,
        object: EncodedTerm,
        properties: Vec<(u64, EncodedTerm)>,
    ) -> Self {
        Self {
            subject,
            predicate,
            object,
            properties,
        }
    }

    pub fn subject(&self) -> u64 {
        self.subject
    }

    pub fn predicate(&self) -> u64 {
        self.predicate
    }

    pub fn object(&self) -> &EncodedTerm {
        &self.object
    }

    pub fn properties(&self) -> &[(u64, EncodedTerm)] {
        &self.properties
    }

    pub fn properties_mut(&mut self) -> &mut Vec<(u64, EncodedTerm)> {
        &mut self.properties
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut w = vec![FORMAT];
        w.extend_from_slice(&self.subject.to_be_bytes());
        w.extend_from_slice(&self.predicate.to_be_bytes());
        w.extend_from_slice(self.object.as_bytes());
        w.extend_from_slice(&(self.properties.len() as u16).to_be_bytes());
        for (predicate, value) in &self.properties {
            w.extend_from_slice(&predicate.to_be_bytes());
            w.extend_from_slice(value.as_bytes());
        }
        w
    }

    /// The record stored as `bytes` under `key`.
    pub fn decode(key: &[u8], bytes: &[u8]) -> Result<Self, QuadError> {
        let bad = |reason: &str| {
            QuadError::Corrupted(key, format!("unreadable statement record: {reason}"))
        };
        let (format, rest) = bytes.split_first().ok_or_else(|| bad("empty"))?;
        if *format != FORMAT {
            return Err(bad("unknown format"));
        }
        let u64_at = |b| u64_at(key, b);
        let (subject, rest) = u64_at(rest)?;
        let (predicate, rest) = u64_at(rest)?;
        let (object, rest) = EncodedTerm::read(rest).map_err(|e| bad(&e.to_string()))?;
        let (count, mut rest) = rest.split_at_checked(2).ok_or_else(|| bad("ends early"))?;
        let mut properties = Vec::new();
        for _ in 0..u16::from_be_bytes([count[0], count[1]]) {
            let (p, after) = u64_at(rest)?;
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

/// The u64 big-endian `bytes` begin with, and the bytes after it.
fn u64_at<'b>(key: &[u8], bytes: &'b [u8]) -> Result<(u64, &'b [u8]), QuadError> {
    let (head, rest) = bytes
        .split_at_checked(8)
        .ok_or_else(|| QuadError::Corrupted(key, "unreadable statement record: ends early"))?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(head);
    Ok((u64::from_be_bytes(raw), rest))
}
