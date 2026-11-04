/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::ControlFlow;

use oxrdf::{NamedNode, Term};
use ultragraphdb_core_ids::{SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_terms::{EncodedTerm, Hash128};
use ultragraphdb_store_posting::{BIT_COMPLETE_POSTING, CachedList, PostingError};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store, StoreSnapshot};

use crate::errors::quad_error::{QuadError, QuadErrorEnum};
use crate::types::dictionary::held_id;
use crate::types::quad_store::QuadStore;
use crate::types::statement_record::StatementRecord;
use crate::types::statistics::Statistics;
use crate::utils::dictionary_keys::{id_key, literal_key};
use crate::utils::postings::posting_object;
use crate::utils::skolem::{is_skolem, skolem_blank_node};

impl<S, C, B> QuadStore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// The ID the dictionary holds for `term`, an IRI or a reifier's name, in `namespace`, if any: what the store has
    /// committed, never an ID a running execution created.
    pub fn id_of(&self, namespace: u64, term: &str) -> Result<Option<SystemId>, QuadError> {
        held_id(self.store(), namespace, term)
    }

    /// The text `id` names, and whether it skolemizes a blank node: a `Resource` ID named by a skolem IRI under the
    /// namespace's base. A missing entry is corruption naming its key.
    pub fn iri_of(&self, namespace: u64, id: SystemId) -> Result<(String, bool), QuadError> {
        let key = id_key(namespace, id);
        let value = self
            .store()
            .get_versionless(&key)?
            .ok_or_else(|| QuadError::corrupted(&key, "a system ID has no dictionary entry"))?;
        let text = String::from_utf8(value)
            .map_err(|_| QuadError::corrupted(&key, "a term is not UTF-8"))?;
        let blank = id.type_prefix() == TypePrefix::RESOURCE && is_skolem(namespace, &text);
        Ok((text, blank))
    }

    /// The string `hash` names. A missing entry is corruption naming its key.
    pub fn string_of(&self, namespace: u64, hash: Hash128) -> Result<String, QuadError> {
        let key = literal_key(namespace, &hash);
        let value = self.store().get_versionless(&key)?.ok_or_else(|| {
            QuadError::corrupted(&key, "a literal's hash has no dictionary entry")
        })?;
        String::from_utf8(value).map_err(|_| QuadError::corrupted(&key, "a string is not UTF-8"))
    }

    /// The RDF term `encoded` names: an IRI, a skolemized blank node presented as a blank node again, labelled by its
    /// skolem IRI's bucket and random digits, or a literal.
    pub fn term_of(&self, namespace: u64, encoded: &EncodedTerm) -> Result<Term, QuadError> {
        if let Some(id) = encoded.as_id() {
            let (text, blank) = self.iri_of(namespace, id)?;
            return Ok(if blank {
                skolem_blank_node(&text).into()
            } else {
                NamedNode::new_unchecked(text).into()
            });
        }
        let literal = encoded.view()?.to_literal(
            |id| Ok::<_, QuadError>(NamedNode::new_unchecked(self.iri_of(namespace, id)?.0)),
            |hash| self.string_of(namespace, hash),
        )?;
        Ok(literal.into())
    }

    /// The objects the list of `key` holds at `read_ts`, each with its statement IDs.
    pub fn objects(
        &self,
        key: &[u8],
        read_ts: u64,
    ) -> Result<Vec<(EncodedTerm, Vec<SystemId>)>, QuadError> {
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
                    QuadErrorEnum::Corrupted { reason, .. } => QuadError::corrupted(key, reason),
                    QuadErrorEnum::Store(_)
                    | QuadErrorEnum::Posting(_)
                    | QuadErrorEnum::Term(_)
                    | QuadErrorEnum::Key(_)
                    | QuadErrorEnum::Mapping(_)
                    | QuadErrorEnum::Catalog(_)
                    | QuadErrorEnum::Id(_)
                    | QuadErrorEnum::NoSequence { .. }
                    | QuadErrorEnum::PendingFull { .. }
                    | QuadErrorEnum::NoOwnedBucket { .. }
                    | QuadErrorEnum::SkolemDraws { .. }
                    | QuadErrorEnum::NotOwned { .. }
                    | QuadErrorEnum::UnknownSkolem { .. } => e,
                })
            })
            .collect()
    }

    /// The statistics of predicate `predicate` in `graph` at `read_ts`: the newest complete version and every delta
    /// above it, merged.
    pub fn statistics(
        &self,
        namespace: u64,
        graph: Graph,
        predicate: SystemId,
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
        graph: Graph,
        id: SystemId,
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
