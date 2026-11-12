/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::{EncodedTerm, Hash128, TermErrorEnum};

use crate::errors::quad_key_error::QuadKeyError;
use crate::types::count_direction::CountDirection;
use crate::types::dictionary_entry::DictionaryEntry;
use crate::types::graph::Graph;
use crate::types::keyspace::Keyspace;
use crate::types::quad_key::{GROUP_MAX, QuadKey};
use crate::types::split_start::SplitStart;

/// The kind byte of a split start that is a fingerprint.
pub(crate) const START_FINGERPRINT: u8 = 0x01;
/// The kind byte of a split start that is an ID.
pub(crate) const START_ID: u8 = 0x02;
/// The length of a split start that is an ID, its kind byte included.
const ID_START_LEN: usize = 1 + SystemId::BYTES;

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
        let (bytes, end) = self
            .at
            .checked_add(N)
            .and_then(|end| {
                Some((
                    <[u8; N]>::try_from(self.fields.get(self.at..end)?).ok()?,
                    end,
                ))
            })
            .ok_or_else(|| QuadKeyError::truncated(self.key))?;
        self.at = end;
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
        // An IRI or a blank node is named by its ID, which must name an entity.
        if !term.is_literal() {
            let id = term.as_bytes().get(1..).unwrap_or_default();
            if let Err(error) = SystemId::decode(id) {
                return Err(QuadKeyError::invalid_id(self.key, "object", error));
            }
        }
        debug_assert!(after.len() < rest.len(), "an object takes at least its tag");
        self.at = self
            .fields
            .len()
            .checked_sub(after.len())
            .ok_or_else(|| QuadKeyError::truncated(self.key))?;
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

    /// The fields after the tenant of a key of `keyspace`.
    fn fields(&mut self, keyspace: Keyspace, tenant: u64) -> Result<QuadKey, QuadKeyError> {
        match keyspace {
            Keyspace::Catalog => Ok(QuadKey::Catalog {
                tenant,
                kind: self.u8()?,
                id: self.id("id")?,
            }),
            Keyspace::Forward => Ok(QuadKey::Forward {
                tenant,
                graph: self.graph()?,
                predicate: self.id("predicate")?,
                subject: self.id("subject")?,
            }),
            Keyspace::Reverse => Ok(QuadKey::Reverse {
                tenant,
                graph: self.graph()?,
                predicate: self.id("predicate")?,
                object: self.object()?,
            }),
            Keyspace::SubjectPredicates => Ok(QuadKey::SubjectPredicates {
                tenant,
                graph: self.graph()?,
                subject: self.id("subject")?,
            }),
            Keyspace::ObjectPredicates => Ok(QuadKey::ObjectPredicates {
                tenant,
                graph: self.graph()?,
                object: self.object()?,
            }),
            Keyspace::ValueIndex => self.value_index(tenant),
            Keyspace::CountIndex => self.count_index(tenant),
            Keyspace::VectorIndex => self.vector_index(tenant),
            Keyspace::Statements => Ok(QuadKey::Statement {
                tenant,
                graph: self.graph()?,
                statement: self.id("statement ID")?,
            }),
            Keyspace::Statistics => Ok(QuadKey::Statistics {
                tenant,
                graph: self.graph()?,
                predicate: self.id("predicate")?,
            }),
            Keyspace::Marks => Ok(QuadKey::Mark {
                tenant,
                entity: self.id("entity")?,
                group: self.group()?,
            }),
            Keyspace::Referrers => Ok(QuadKey::Referrers {
                tenant,
                entity: self.id("entity")?,
            }),
            Keyspace::Dictionary => self.dictionary(tenant),
        }
    }

    fn value_index(&mut self, tenant: u64) -> Result<QuadKey, QuadKeyError> {
        Ok(QuadKey::ValueIndex {
            tenant,
            graph: self.graph()?,
            predicate: self.id("predicate")?,
            tokenizer: self.u8()?,
            token: self.token(),
        })
    }

    fn count_index(&mut self, tenant: u64) -> Result<QuadKey, QuadKeyError> {
        Ok(QuadKey::CountIndex {
            tenant,
            graph: self.graph()?,
            predicate: self.id("predicate")?,
            direction: self.direction()?,
            count: self.u32()?,
        })
    }

    /// A dictionary key's fields: its kind byte, then a term's hash, an ID or a literal's hash.
    fn dictionary(&mut self, tenant: u64) -> Result<QuadKey, QuadKeyError> {
        let entry = match self.u8()? {
            0x01 => DictionaryEntry::Term(Hash128::from_bytes(self.array::<16>()?)),
            0x02 => DictionaryEntry::Id(self.id("ID")?),
            0x03 => DictionaryEntry::Literal(Hash128::from_bytes(self.array::<16>()?)),
            kind => return Err(QuadKeyError::unknown_kind(self.key, kind)),
        };
        Ok(QuadKey::Dictionary { tenant, entry })
    }

    fn vector_index(&mut self, tenant: u64) -> Result<QuadKey, QuadKeyError> {
        Ok(QuadKey::VectorIndex {
            tenant,
            graph: self.graph()?,
            predicate: self.id("predicate")?,
            list: self.u8()?,
            node: self.id("node")?,
        })
    }

    /// The split start after the fields: its kind byte, then 8 bytes of a fingerprint or 16 of an ID.
    fn split_start(&mut self) -> Result<SplitStart, QuadKeyError> {
        let start = match self.u8()? {
            START_FINGERPRINT => SplitStart::Fingerprint(self.u64()?),
            START_ID => SplitStart::Id(self.id("split start")?),
            kind => return Err(QuadKeyError::unknown_kind(self.key, kind)),
        };
        debug_assert!(
            self.at <= self.fields.len(),
            "a start is read within the key"
        );
        Ok(start)
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
    /// its start's kind byte and the start, kind 1 and 8 bytes of a fingerprint or kind 2 and 16 of an ID, and a
    /// value-index key's start is its last 17 bytes, always an ID. Nothing it is given makes it panic.
    ///
    /// # Errors
    ///
    /// Each names the key's bytes: [`QuadKeyError::truncated`] when it ends early, [`QuadKeyError::trailing_bytes`]
    /// when it runs on past its fields or its split start, [`QuadKeyError::unknown_keyspace`] for an unknown keyspace
    /// byte or the split flag on a keyspace without lists, [`QuadKeyError::invalid_id`] for an ID that is not a valid
    /// system ID (the default graph's 16 zero bytes excepted), [`QuadKeyError::unknown_direction`],
    /// [`QuadKeyError::bad_object`] and [`QuadKeyError::vector_object`] for a count key's direction and an object it
    /// cannot take, [`QuadKeyError::group_range`] for a mark's group of 2^18 or more,
    /// [`QuadKeyError::fingerprint_start`] for a value-index key split at a fingerprint, and
    /// [`QuadKeyError::unknown_kind`] for a split start's or a dictionary key's kind byte the layout does not define.
    pub fn parse(key: &[u8]) -> Result<(QuadKey, Option<SplitStart>), QuadKeyError> {
        let first = *key.first().ok_or_else(|| QuadKeyError::truncated(key))?;
        let (keyspace, split) =
            Keyspace::from_byte(first).ok_or_else(|| QuadKeyError::unknown_keyspace(key))?;
        if split && !keyspace.is_list() {
            return Err(QuadKeyError::unknown_keyspace(key));
        }
        // A value-index key's token runs to its end, so a split one's start, always an ID, is its last 17 bytes: the
        // kind byte and the ID, read apart.
        let (fields, id_start) = if split && keyspace == Keyspace::ValueIndex {
            let at = key
                .len()
                .checked_sub(ID_START_LEN)
                .ok_or_else(|| QuadKeyError::truncated(key))?;
            let (fields, start) = key
                .split_at_checked(at)
                .ok_or_else(|| QuadKeyError::truncated(key))?;
            (fields, Some(start))
        } else {
            (key, None)
        };
        let mut reader = Reader { key, fields, at: 1 };
        let tenant = reader.u64()?;
        let parsed = reader.fields(keyspace, tenant)?;
        let start = match (split, id_start) {
            (false, _) => None,
            (true, Some(start)) => Some(value_index_start(key, start)?),
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

/// A split value-index key's start, `start`, its last 17 bytes: the ID kind byte and the ID.
fn value_index_start(key: &[u8], start: &[u8]) -> Result<SplitStart, QuadKeyError> {
    debug_assert_eq!(
        start.len(),
        ID_START_LEN,
        "the start is the key's last 17 bytes"
    );
    match start.split_first() {
        Some((&START_ID, id)) => SystemId::decode(id)
            .map(SplitStart::Id)
            .map_err(|e| QuadKeyError::invalid_id(key, "split start", e)),
        Some((&START_FINGERPRINT, _)) => Err(QuadKeyError::fingerprint_start(key)),
        Some((&kind, _)) => Err(QuadKeyError::unknown_kind(key, kind)),
        None => Err(QuadKeyError::truncated(key)),
    }
}
