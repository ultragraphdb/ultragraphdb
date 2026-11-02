/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::{EncodedTerm, TermErrorEnum};

use crate::errors::quad_key_error::QuadKeyError;
use crate::types::count_direction::CountDirection;
use crate::types::graph::Graph;
use crate::types::keyspace::Keyspace;
use crate::types::quad_key::{GROUP_MAX, QuadKey};
use crate::types::split_start::SplitStart;

/// The length of a fingerprint start.
const FINGERPRINT_LEN: usize = 8;

/// The fields of a key being read, front to back: `key` is the whole key, for the errors, and `fields` the part of it
/// a key's fields are read from, of which `at` bytes are read.
struct Reader<'a> {
    key: &'a [u8],
    fields: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// The next `N` bytes.
    fn array<const N: usize>(&mut self) -> Result<[u8; N], QuadKeyError> {
        let bytes = self
            .at
            .checked_add(N)
            .and_then(|end| self.fields.get(self.at..end))
            .and_then(|bytes| <[u8; N]>::try_from(bytes).ok())
            .ok_or_else(|| QuadKeyError::truncated(self.key))?;
        self.at += N;
        Ok(bytes)
    }

    fn u8(&mut self) -> Result<u8, QuadKeyError> {
        let [byte] = self.array::<1>()?;
        Ok(byte)
    }

    fn u32(&mut self) -> Result<u32, QuadKeyError> {
        Ok(u32::from_be_bytes(self.array::<4>()?))
    }

    fn u64(&mut self) -> Result<u64, QuadKeyError> {
        Ok(u64::from_be_bytes(self.array::<8>()?))
    }

    /// A system ID, as `field` names it.
    fn id(&mut self, field: &'static str) -> Result<SystemId, QuadKeyError> {
        let bytes = self.array::<{ SystemId::BYTES }>()?;
        SystemId::decode(&bytes).map_err(|e| QuadKeyError::invalid_id(self.key, field, e))
    }

    /// A graph: the default graph's 16 zero bytes, or a named graph's system ID.
    fn graph(&mut self) -> Result<Graph, QuadKeyError> {
        let bytes = self.array::<{ SystemId::BYTES }>()?;
        if bytes == [0; SystemId::BYTES] {
            return Ok(Graph::Default);
        }
        SystemId::decode(&bytes)
            .map(Graph::Named)
            .map_err(|e| QuadKeyError::invalid_id(self.key, "graph", e))
    }

    /// The bytes not read yet.
    fn remaining(&self) -> &'a [u8] {
        self.fields.get(self.at..).unwrap_or_default()
    }

    fn object(&mut self) -> Result<EncodedTerm, QuadKeyError> {
        let rest = self.remaining();
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
        debug_assert!(after.len() < rest.len(), "an object takes at least its tag");
        self.at = self.fields.len() - after.len();
        Ok(term)
    }

    /// The token: every byte not read yet.
    fn token(&mut self) -> Vec<u8> {
        let token = self.remaining().to_vec();
        self.at = self.fields.len();
        token
    }

    fn direction(&mut self) -> Result<CountDirection, QuadKeyError> {
        CountDirection::from_byte(self.u8()?)
            .ok_or_else(|| QuadKeyError::unknown_direction(self.key))
    }

    /// A mark's bucket group: 3 big-endian bytes holding 18 bits.
    fn group(&mut self) -> Result<u32, QuadKeyError> {
        let [high, middle, low] = self.array::<3>()?;
        let group = u32::from_be_bytes([0, high, middle, low]);
        if group > GROUP_MAX {
            return Err(QuadKeyError::group_range(self.key, group));
        }
        Ok(group)
    }

    /// The fields after the namespace of a key of `keyspace`.
    fn fields(&mut self, keyspace: Keyspace, namespace: u64) -> Result<QuadKey, QuadKeyError> {
        match keyspace {
            Keyspace::Catalog => Ok(QuadKey::Catalog {
                namespace,
                kind: self.u8()?,
                id: self.id("id")?,
            }),
            Keyspace::Forward => Ok(QuadKey::Forward {
                namespace,
                graph: self.graph()?,
                predicate: self.id("predicate")?,
                subject: self.id("subject")?,
            }),
            Keyspace::Reverse => Ok(QuadKey::Reverse {
                namespace,
                graph: self.graph()?,
                predicate: self.id("predicate")?,
                object: self.object()?,
            }),
            Keyspace::SubjectPredicates => Ok(QuadKey::SubjectPredicates {
                namespace,
                graph: self.graph()?,
                subject: self.id("subject")?,
            }),
            Keyspace::ObjectPredicates => Ok(QuadKey::ObjectPredicates {
                namespace,
                graph: self.graph()?,
                object: self.object()?,
            }),
            Keyspace::ValueIndex => self.value_index(namespace),
            Keyspace::CountIndex => self.count_index(namespace),
            Keyspace::VectorIndex => self.vector_index(namespace),
            Keyspace::Statements => Ok(QuadKey::Statement {
                namespace,
                graph: self.graph()?,
                statement: self.id("statement ID")?,
            }),
            Keyspace::Statistics => Ok(QuadKey::Statistics {
                namespace,
                graph: self.graph()?,
                predicate: self.id("predicate")?,
            }),
            Keyspace::Marks => Ok(QuadKey::Mark {
                namespace,
                entity: self.id("entity")?,
                group: self.group()?,
            }),
            Keyspace::Referrers => Ok(QuadKey::Referrers {
                namespace,
                entity: self.id("entity")?,
            }),
        }
    }

    fn value_index(&mut self, namespace: u64) -> Result<QuadKey, QuadKeyError> {
        Ok(QuadKey::ValueIndex {
            namespace,
            graph: self.graph()?,
            predicate: self.id("predicate")?,
            tokenizer: self.u8()?,
            token: self.token(),
        })
    }

    fn count_index(&mut self, namespace: u64) -> Result<QuadKey, QuadKeyError> {
        Ok(QuadKey::CountIndex {
            namespace,
            graph: self.graph()?,
            predicate: self.id("predicate")?,
            direction: self.direction()?,
            count: self.u32()?,
        })
    }

    fn vector_index(&mut self, namespace: u64) -> Result<QuadKey, QuadKeyError> {
        Ok(QuadKey::VectorIndex {
            namespace,
            graph: self.graph()?,
            predicate: self.id("predicate")?,
            list: self.u8()?,
            node: self.id("node")?,
        })
    }

    /// The split start after the fields: 16 bytes an ID, 8 a fingerprint.
    fn split_start(&mut self) -> Result<SplitStart, QuadKeyError> {
        match self.remaining().len() {
            SystemId::BYTES => Ok(SplitStart::Id(self.id("split start")?)),
            FINGERPRINT_LEN => Ok(SplitStart::Fingerprint(self.u64()?)),
            len if len > SystemId::BYTES => Err(QuadKeyError::trailing_bytes(self.key)),
            _ => Err(QuadKeyError::truncated(self.key)),
        }
    }

    fn done(&self) -> Result<(), QuadKeyError> {
        debug_assert!(
            self.at <= self.fields.len(),
            "no field is read past the end"
        );
        if self.at == self.fields.len() {
            Ok(())
        } else {
            Err(QuadKeyError::trailing_bytes(self.key))
        }
    }
}

impl QuadKey {
    /// The fields of `key`, and the start of the part when it is a split key: what follows a split key's fields is
    /// its start, 16 bytes an ID and 8 a fingerprint, and a value-index key's start is the last 16 bytes. Nothing it
    /// is given makes it panic.
    ///
    /// # Errors
    ///
    /// Each names the key's bytes: [`QuadKeyError::truncated`] when it ends early, [`QuadKeyError::trailing_bytes`]
    /// when it runs on past its fields or a split start is neither 8 nor 16 bytes, [`QuadKeyError::unknown_keyspace`]
    /// for an unknown keyspace byte or the split flag on a keyspace without lists, [`QuadKeyError::invalid_id`] for an
    /// ID that is not a valid system ID (the default graph's 16 zero bytes excepted),
    /// [`QuadKeyError::unknown_direction`], [`QuadKeyError::bad_object`] and [`QuadKeyError::vector_object`] for a
    /// count key's direction and an object it cannot take, and [`QuadKeyError::group_range`] for a mark's group of
    /// 2^18 or more.
    pub fn parse(key: &[u8]) -> Result<(QuadKey, Option<SplitStart>), QuadKeyError> {
        let first = *key.first().ok_or_else(|| QuadKeyError::truncated(key))?;
        let (keyspace, split) =
            Keyspace::from_byte(first).ok_or_else(|| QuadKeyError::unknown_keyspace(key))?;
        if split && !keyspace.is_list() {
            return Err(QuadKeyError::unknown_keyspace(key));
        }
        // A value-index key's token runs to its end, so a split one's start is its last 16 bytes, read apart.
        let (fields, id_start) = if split && keyspace == Keyspace::ValueIndex {
            let at = key
                .len()
                .checked_sub(SystemId::BYTES)
                .ok_or_else(|| QuadKeyError::truncated(key))?;
            let (fields, start) = key
                .split_at_checked(at)
                .ok_or_else(|| QuadKeyError::truncated(key))?;
            (fields, Some(start))
        } else {
            (key, None)
        };
        let mut reader = Reader { key, fields, at: 1 };
        let namespace = reader.u64()?;
        let parsed = reader.fields(keyspace, namespace)?;
        let start = match (split, id_start) {
            (false, _) => None,
            (true, Some(start)) => {
                Some(SplitStart::Id(SystemId::decode(start).map_err(|e| {
                    QuadKeyError::invalid_id(key, "split start", e)
                })?))
            }
            (true, None) => Some(reader.split_start()?),
        };
        reader.done()?;
        debug_assert_eq!(
            parsed.keyspace(),
            keyspace,
            "a key parses into its keyspace"
        );
        Ok((parsed, start))
    }
}
