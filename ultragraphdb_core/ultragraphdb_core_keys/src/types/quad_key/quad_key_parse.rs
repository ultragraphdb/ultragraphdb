/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::{EncodedTerm, TermErrorEnum};

use crate::errors::quad_key_error::QuadKeyError;
use crate::types::count_direction::CountDirection;
use crate::types::keyspace::Keyspace;
use crate::types::quad_key::QuadKey;

/// The fields of a key being read, front to back.
struct Reader<'a> {
    key: &'a [u8],
    at: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, QuadKeyError> {
        let byte = *self.key[..self.end]
            .get(self.at)
            .ok_or_else(|| QuadKeyError::truncated(self.key))?;
        self.at += 1;
        Ok(byte)
    }

    fn u64(&mut self) -> Result<u64, QuadKeyError> {
        let bytes = self.key[..self.end]
            .get(self.at..self.at + 8)
            .ok_or_else(|| QuadKeyError::truncated(self.key))?;
        self.at += 8;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        Ok(u64::from_be_bytes(raw))
    }

    /// A UID that may not be 0, as `field` names it.
    fn uid(&mut self, field: &'static str) -> Result<u64, QuadKeyError> {
        let uid = self.u64()?;
        if uid == 0 {
            return Err(QuadKeyError::zero_uid(self.key, field));
        }
        Ok(uid)
    }

    fn u32(&mut self) -> Result<u32, QuadKeyError> {
        let bytes = self.key[..self.end]
            .get(self.at..self.at + 4)
            .ok_or_else(|| QuadKeyError::truncated(self.key))?;
        self.at += 4;
        let mut raw = [0u8; 4];
        raw.copy_from_slice(bytes);
        Ok(u32::from_be_bytes(raw))
    }

    fn object(&mut self) -> Result<EncodedTerm, QuadKeyError> {
        let rest = &self.key[self.at..self.end];
        if rest.is_empty() {
            return Err(QuadKeyError::truncated(self.key));
        }
        let (term, after) = EncodedTerm::read(rest).map_err(|e| match e.0 {
            TermErrorEnum::Truncated { .. } => QuadKeyError::truncated(self.key),
            TermErrorEnum::UnknownTag { .. }
            | TermErrorEnum::UnknownDatatype { .. }
            | TermErrorEnum::Malformed { .. }
            | TermErrorEnum::InvalidVector { .. }
            | TermErrorEnum::NotALiteral { .. } => QuadKeyError::bad_object(self.key, e),
        })?;
        if term.is_vector() {
            return Err(QuadKeyError::vector_object(self.key));
        }
        if term.as_uid() == Some(0) {
            return Err(QuadKeyError::zero_uid(self.key, "object"));
        }
        self.at = self.end - after.len();
        Ok(term)
    }

    fn rest(&mut self) -> Vec<u8> {
        let rest = self.key[self.at..self.end].to_vec();
        self.at = self.end;
        rest
    }

    /// The fields after the namespace of a key of `keyspace`.
    fn fields(&mut self, keyspace: Keyspace, namespace: u64) -> Result<QuadKey, QuadKeyError> {
        Ok(match keyspace {
            Keyspace::Catalog => QuadKey::Catalog {
                namespace,
                kind: self.u8()?,
                uid: self.uid("uid")?,
            },
            Keyspace::Forward => QuadKey::Forward {
                namespace,
                graph: self.u64()?,
                predicate: self.uid("predicate")?,
                subject: self.uid("subject")?,
            },
            Keyspace::Reverse => QuadKey::Reverse {
                namespace,
                graph: self.u64()?,
                predicate: self.uid("predicate")?,
                object: self.object()?,
            },
            Keyspace::SubjectPredicates => QuadKey::SubjectPredicates {
                namespace,
                graph: self.u64()?,
                subject: self.uid("subject")?,
            },
            Keyspace::ObjectPredicates => QuadKey::ObjectPredicates {
                namespace,
                graph: self.u64()?,
                object: self.object()?,
            },
            Keyspace::ValueIndex => QuadKey::ValueIndex {
                namespace,
                graph: self.u64()?,
                predicate: self.uid("predicate")?,
                tokenizer: self.u8()?,
                token: self.rest(),
            },
            Keyspace::CountIndex => {
                let graph = self.u64()?;
                let predicate = self.uid("predicate")?;
                let direction = CountDirection::from_byte(self.u8()?)
                    .ok_or_else(|| QuadKeyError::unknown_direction(self.key))?;
                QuadKey::CountIndex {
                    namespace,
                    graph,
                    predicate,
                    direction,
                    count: self.u32()?,
                }
            }
            Keyspace::VectorIndex => QuadKey::VectorIndex {
                namespace,
                graph: self.u64()?,
                predicate: self.uid("predicate")?,
                list: self.u8()?,
                node: self.uid("node")?,
            },
            Keyspace::Statements => QuadKey::Statement {
                namespace,
                graph: self.u64()?,
                statement: self.uid("statement ID")?,
            },
            Keyspace::Statistics => QuadKey::Statistics {
                namespace,
                graph: self.u64()?,
                predicate: self.uid("predicate")?,
            },
        })
    }

    fn done(&self) -> Result<(), QuadKeyError> {
        if self.at == self.end {
            Ok(())
        } else {
            Err(QuadKeyError::trailing_bytes(self.key))
        }
    }
}

impl QuadKey {
    /// The fields of `key`, and the start of the part when it is a split key. Refused, naming the key's bytes, when
    /// it ends early, runs on past its fields, names a UID of 0 other than the default graph, a statement ID of 0,
    /// or an object it cannot read; nothing it is given makes it panic.
    pub fn parse(key: &[u8]) -> Result<(QuadKey, Option<u64>), QuadKeyError> {
        let first = *key.first().ok_or_else(|| QuadKeyError::truncated(key))?;
        let (keyspace, split) =
            Keyspace::from_byte(first).ok_or_else(|| QuadKeyError::unknown_keyspace(key))?;
        if split && !keyspace.is_list() {
            return Err(QuadKeyError::unknown_keyspace(key));
        }
        let (end, start) = if split {
            let end = key
                .len()
                .checked_sub(8)
                .filter(|end| *end >= 9)
                .ok_or_else(|| QuadKeyError::truncated(key))?;
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&key[end..]);
            (end, Some(u64::from_be_bytes(raw)))
        } else {
            (key.len(), None)
        };
        let mut r = Reader { key, at: 1, end };
        let namespace = r.u64()?;
        let parsed = r.fields(keyspace, namespace)?;
        r.done()?;
        Ok((parsed, start))
    }
}
