/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_terms::EncodedTerm;

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
            .ok_or_else(|| QuadKeyError::Truncated(self.key))?;
        self.at += 1;
        Ok(byte)
    }

    fn u64(&mut self) -> Result<u64, QuadKeyError> {
        let bytes = self.key[..self.end]
            .get(self.at..self.at + 8)
            .ok_or_else(|| QuadKeyError::Truncated(self.key))?;
        self.at += 8;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        Ok(u64::from_be_bytes(raw))
    }

    /// A UID that may not be 0, as `field` names it.
    fn uid(&mut self, field: &'static str) -> Result<u64, QuadKeyError> {
        let uid = self.u64()?;
        if uid == 0 {
            return Err(QuadKeyError::ZeroUid(self.key, field));
        }
        Ok(uid)
    }

    fn u32(&mut self) -> Result<u32, QuadKeyError> {
        let bytes = self.key[..self.end]
            .get(self.at..self.at + 4)
            .ok_or_else(|| QuadKeyError::Truncated(self.key))?;
        self.at += 4;
        let mut raw = [0u8; 4];
        raw.copy_from_slice(bytes);
        Ok(u32::from_be_bytes(raw))
    }

    fn object(&mut self) -> Result<EncodedTerm, QuadKeyError> {
        let rest = &self.key[self.at..self.end];
        if rest.is_empty() {
            return Err(QuadKeyError::Truncated(self.key));
        }
        let (term, after) = EncodedTerm::read(rest).map_err(|e| match e.0 {
            ultragraphdb_core_terms::TermErrorEnum::Truncated { .. } => {
                QuadKeyError::Truncated(self.key)
            }
            _ => QuadKeyError::BadObject(self.key, e),
        })?;
        if term.is_vector() {
            return Err(QuadKeyError::VectorObject(self.key));
        }
        if term.as_uid() == Some(0) {
            return Err(QuadKeyError::ZeroUid(self.key, "object"));
        }
        self.at = self.end - after.len();
        Ok(term)
    }

    fn rest(&mut self) -> Vec<u8> {
        let rest = self.key[self.at..self.end].to_vec();
        self.at = self.end;
        rest
    }

    fn done(&self) -> Result<(), QuadKeyError> {
        if self.at == self.end {
            Ok(())
        } else {
            Err(QuadKeyError::TrailingBytes(self.key))
        }
    }
}

impl QuadKey {
    /// The fields of `key`, and the start of the part when it is a split key. Refused, naming the key's bytes, when
    /// it ends early, runs on past its fields, names a UID of 0 other than the default graph, a statement ID of 0,
    /// or an object it cannot read; nothing it is given makes it panic.
    pub fn parse(key: &[u8]) -> Result<(QuadKey, Option<u64>), QuadKeyError> {
        let first = *key.first().ok_or_else(|| QuadKeyError::Truncated(key))?;
        let (keyspace, split) =
            Keyspace::from_byte(first).ok_or_else(|| QuadKeyError::UnknownKeyspace(key))?;
        if split && !keyspace.is_list() {
            return Err(QuadKeyError::UnknownKeyspace(key));
        }
        let (end, start) = if split {
            let end = key
                .len()
                .checked_sub(8)
                .filter(|end| *end >= 9)
                .ok_or_else(|| QuadKeyError::Truncated(key))?;
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&key[end..]);
            (end, Some(u64::from_be_bytes(raw)))
        } else {
            (key.len(), None)
        };
        let mut r = Reader { key, at: 1, end };
        let namespace = r.u64()?;
        let parsed = match keyspace {
            Keyspace::Catalog => QuadKey::Catalog {
                namespace,
                kind: r.u8()?,
                uid: r.uid("uid")?,
            },
            Keyspace::Forward => QuadKey::Forward {
                namespace,
                graph: r.u64()?,
                predicate: r.uid("predicate")?,
                subject: r.uid("subject")?,
            },
            Keyspace::Reverse => QuadKey::Reverse {
                namespace,
                graph: r.u64()?,
                predicate: r.uid("predicate")?,
                object: r.object()?,
            },
            Keyspace::SubjectPredicates => QuadKey::SubjectPredicates {
                namespace,
                graph: r.u64()?,
                subject: r.uid("subject")?,
            },
            Keyspace::ObjectPredicates => QuadKey::ObjectPredicates {
                namespace,
                graph: r.u64()?,
                object: r.object()?,
            },
            Keyspace::ValueIndex => QuadKey::ValueIndex {
                namespace,
                graph: r.u64()?,
                predicate: r.uid("predicate")?,
                tokenizer: r.u8()?,
                token: r.rest(),
            },
            Keyspace::CountIndex => {
                let graph = r.u64()?;
                let predicate = r.uid("predicate")?;
                let direction = CountDirection::from_byte(r.u8()?)
                    .ok_or_else(|| QuadKeyError::UnknownDirection(key))?;
                QuadKey::CountIndex {
                    namespace,
                    graph,
                    predicate,
                    direction,
                    count: r.u32()?,
                }
            }
            Keyspace::VectorIndex => QuadKey::VectorIndex {
                namespace,
                graph: r.u64()?,
                predicate: r.uid("predicate")?,
                list: r.u8()?,
                node: r.uid("node")?,
            },
            Keyspace::Statements => QuadKey::Statement {
                namespace,
                graph: r.u64()?,
                statement: r.uid("statement ID")?,
            },
            Keyspace::Statistics => QuadKey::Statistics {
                namespace,
                graph: r.u64()?,
                predicate: r.uid("predicate")?,
            },
        };
        r.done()?;
        Ok((parsed, start))
    }
}
