/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use oxrdf::{BlankNode, NamedNode, Term};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_store_posting::{BIT_COMPLETE_POSTING, CachedList, PostingError};
use ultragraphdb_traits_store::{ListCache, Store, StoreSnapshot};

use crate::errors::quad_error::{QuadError, QuadErrorEnum};
use crate::types::quad_store::QuadStore;
use crate::types::statement_record::StatementRecord;
use crate::types::statistics::Statistics;
use crate::types::write_terms::decode_uid;
use crate::utils::dictionary_keys::{iri_key, string_key, uid_key};
use crate::utils::postings::posting_object;

impl<S, C> QuadStore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The UID the dictionary holds for `iri` in `namespace`, if any.
    pub fn uid_of(&self, namespace: u64, iri: &str) -> Result<Option<u64>, QuadError> {
        let key = iri_key(namespace, iri);
        self.store()
            .get_versionless(&key)?
            .map(|v| decode_uid(&key, &v))
            .transpose()
    }

    /// The IRI `uid` names, and whether it skolemizes a blank node. A missing entry is corruption naming its key.
    pub fn iri_of(&self, namespace: u64, uid: u64) -> Result<(String, bool), QuadError> {
        let key = uid_key(namespace, uid);
        let value = self.store().get_versionless(&key)?.ok_or_else(|| {
            QuadError::Corrupted(&key, format!("UID {uid} has no dictionary entry"))
        })?;
        let (flag, iri) = value
            .split_first()
            .ok_or_else(|| QuadError::Corrupted(&key, "an empty UID entry"))?;
        let iri = String::from_utf8(iri.to_vec())
            .map_err(|_| QuadError::Corrupted(&key, "an IRI is not UTF-8"))?;
        Ok((iri, *flag == 1))
    }

    /// The string `hash` names. A missing entry is corruption naming its key.
    pub fn string_of(&self, namespace: u64, hash: Hash128) -> Result<String, QuadError> {
        let key = string_key(namespace, &hash);
        let value = self.store().get_versionless(&key)?.ok_or_else(|| {
            QuadError::Corrupted(&key, "a literal's hash has no dictionary entry")
        })?;
        String::from_utf8(value).map_err(|_| QuadError::Corrupted(&key, "a string is not UTF-8"))
    }

    /// The RDF term `encoded` names: an IRI, a skolemized blank node presented as a blank node again, or a literal.
    pub fn term_of(&self, namespace: u64, encoded: &EncodedTerm) -> Result<Term, QuadError> {
        if let Some(uid) = encoded.as_uid() {
            let (iri, skolem) = self.iri_of(namespace, uid)?;
            return Ok(if skolem {
                BlankNode::new_unchecked(format!("b{uid}")).into()
            } else {
                NamedNode::new_unchecked(iri).into()
            });
        }
        let literal = encoded.view()?.to_literal(
            |uid| Ok::<_, QuadError>(NamedNode::new_unchecked(self.iri_of(namespace, uid)?.0)),
            |hash| self.string_of(namespace, hash),
        )?;
        Ok(literal.into())
    }

    /// The objects the list of `key` holds at `read_ts`, each with its statement IDs.
    pub fn objects(
        &self,
        key: &[u8],
        read_ts: u64,
    ) -> Result<Vec<(EncodedTerm, Vec<u64>)>, QuadError> {
        let list = self.lists().read(key, read_ts)?;
        let mut objects = Vec::new();
        list.iterate(read_ts, 0, |p| {
            match posting_object(key, p) {
                Ok(object) => objects.push((object, p.statement_ids().to_vec())),
                Err(e) => return Err(PostingErrorOr::Quad(e)),
            }
            Ok(ControlFlow::Continue(()))
        })
        .map_err(PostingErrorOr::into_quad)?;
        Ok(objects)
    }

    /// The terms the list of `key` holds at `read_ts`, decoded through the dictionary: a missing entry is corruption
    /// naming the list's key.
    pub fn list_terms(
        &self,
        namespace: u64,
        key: &[u8],
        read_ts: u64,
    ) -> Result<Vec<Term>, QuadError> {
        self.objects(key, read_ts)?
            .iter()
            .map(|(object, _)| {
                self.term_of(namespace, object).map_err(|e| match e.0 {
                    QuadErrorEnum::Corrupted { reason, .. } => QuadError::Corrupted(key, reason),
                    _ => e,
                })
            })
            .collect()
    }

    /// The statistics of predicate `predicate` in `graph` at `read_ts`: the newest complete version and every delta
    /// above it, merged.
    pub fn statistics(
        &self,
        namespace: u64,
        graph: u64,
        predicate: u64,
        read_ts: u64,
    ) -> Result<Statistics, QuadError> {
        let key = QuadKey::Statistics {
            namespace,
            graph,
            predicate,
        }
        .to_bytes();
        let snapshot = self.store().snapshot(read_ts)?;
        let mut total = Statistics::new();
        for item in snapshot.versions(&key) {
            let item = item?;
            total.merge(&Statistics::decode(&key, item.value())?);
            if item.meta() & BIT_COMPLETE_POSTING != 0 {
                break;
            }
        }
        Ok(total)
    }

    /// The record of statement `id` in `graph` at `read_ts`.
    pub fn statement(
        &self,
        namespace: u64,
        graph: u64,
        id: u64,
        read_ts: u64,
    ) -> Result<Option<StatementRecord>, QuadError> {
        let key = QuadKey::Statement {
            namespace,
            graph,
            statement: id,
        }
        .to_bytes();
        self.store()
            .get_at(&key, read_ts)?
            .map(|item| StatementRecord::decode(&key, item.value()))
            .transpose()
    }
}

/// A list iteration's error: the posting crate's, or the quad store's own reading a posting.
enum PostingErrorOr {
    Posting(PostingError),
    Quad(QuadError),
}

impl From<PostingError> for PostingErrorOr {
    fn from(e: PostingError) -> Self {
        Self::Posting(e)
    }
}

impl PostingErrorOr {
    fn into_quad(self) -> QuadError {
        match self {
            Self::Posting(e) => e.into(),
            Self::Quad(e) => e,
        }
    }
}
