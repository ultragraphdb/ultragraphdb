/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, Term, TermRef};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::utils::literal_tokens::literal_tokens;

impl<S, C, B> ChangesetWrite<'_, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// The objects of `subject`'s `predicate` in `graph` as the write sees them, its own writes on top of its
    /// snapshot, decoded into `out`, which is cleared first: the subject's subject-predicates list read whole and
    /// recorded, and the subject's list read whole and recorded only when it names the predicate (partitions,
    /// "Absence is proven by a node's predicate lists"); a subject the write created reads its own writes alone. A
    /// graph, subject or predicate the store does not hold is recorded absent and holds nothing.
    pub fn objects_into(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        out: &mut Vec<Term>,
    ) -> Result<(), ChangesetError> {
        out.clear();
        let Some(graph) = self.find_graph(graph)? else {
            return Ok(());
        };
        let Some(subject) = self
            .find_term(predicate, subject.into())?
            .and_then(|e| e.as_id())
        else {
            return Ok(());
        };
        let Some(predicate) = self.find_iri(predicate)? else {
            return Ok(());
        };
        if !self.write.is_fresh(subject)
            && !self.write.names_predicate(graph, subject, predicate)?
        {
            return Ok(());
        }
        let objects = self.write.objects(graph, subject, predicate)?;
        out.reserve(objects.len());
        for (object, _) in &objects {
            out.push(self.decode(object)?);
        }
        Ok(())
    }

    /// The subjects holding `predicate` to `object` in `graph` as the write sees them, into `out`, cleared first: the
    /// object's object-predicates list read whole and recorded, and its reverse list read whole and recorded only when
    /// it names the predicate; an object the write created reads its own writes alone. A term the store does not hold
    /// holds nothing.
    pub fn subjects_into(
        &mut self,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
        out: &mut Vec<NamedOrBlankNode>,
    ) -> Result<(), ChangesetError> {
        out.clear();
        let Some(graph) = self.find_graph(graph)? else {
            return Ok(());
        };
        let Some(uid) = self.find_iri(predicate)? else {
            return Ok(());
        };
        let Some(object) = self.find_term(predicate, object)? else {
            return Ok(());
        };
        let fresh = object.as_id().is_some_and(|id| self.write.is_fresh(id));
        if !fresh && !self.write.points_with(graph, &object, uid)? {
            return Ok(());
        }
        let subjects = self.write.subjects(graph, uid, &object)?;
        out.reserve(subjects.len());
        for subject in subjects {
            out.push(self.node_of(subject)?);
        }
        Ok(())
    }

    /// The subjects whose `predicate` in `graph` holds `value` by the predicate's `tokenizer` index as the write sees
    /// them, into `out`, cleared first, in ID order: the index list of each of the value's tokens read whole and
    /// recorded, and the subjects every one of them holds; a subject whose list of the predicate the write changed, as
    /// that list holds the value now. A value the tokenizer gives no token holds none.
    ///
    /// # Errors
    ///
    /// [`ChangesetError::no_index`] when the catalog declares no `tokenizer` index on `predicate`; the store's error;
    /// corruption of a list or a dictionary entry.
    pub fn lookup_into(
        &mut self,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        tokenizer: &str,
        value: TermRef<'_>,
        out: &mut Vec<NamedOrBlankNode>,
    ) -> Result<(), ChangesetError> {
        out.clear();
        let refused = || ChangesetError::no_index(predicate.as_str(), tokenizer);
        let uid = self.find_iri(predicate)?.ok_or_else(refused)?;
        let declaration = self.behaviour(uid, predicate);
        let Some(declared) = declaration
            .tokenizers()
            .iter()
            .find(|t| t.as_str() == tokenizer)
        else {
            return Err(refused());
        };
        let Some(graph) = self.find_graph(graph)? else {
            return Ok(());
        };
        let TermRef::Literal(literal) = value else {
            return Ok(());
        };
        let declared = std::slice::from_ref(declared);
        let tokens = literal_tokens(declared, literal)?;
        if tokens.is_empty() {
            return Ok(());
        }
        let mut held: Option<Vec<SystemId>> = None;
        for (identifier, token) in &tokens {
            let key = QuadKey::ValueIndex {
                namespace: self.namespace(),
                graph,
                predicate: uid,
                tokenizer: *identifier,
                token: token.clone(),
            }
            .to_bytes();
            let ids = self.write.ids(&key)?;
            held = Some(match held {
                None => ids,
                Some(mut before) => {
                    before.retain(|id| ids.binary_search(id).is_ok());
                    before
                }
            });
        }
        let mut held = held.unwrap_or_default();
        // The write's own changes keep no index until it finishes: a subject whose list of the predicate it changed
        // holds the value as its list holds it now.
        self.own_holders(graph, uid, declared, &tokens, &mut held)?;
        held.sort_unstable();
        out.reserve(held.len());
        for subject in held {
            out.push(self.node_of(subject)?);
        }
        Ok(())
    }

    /// Replaces in `held` each subject whose list of `predicate` in `graph` the write changed by whether that list
    /// holds a literal giving every one of `tokens` under `declared` now.
    fn own_holders(
        &mut self,
        graph: Graph,
        predicate: SystemId,
        declared: &[String],
        tokens: &BTreeSet<(u8, Vec<u8>)>,
        held: &mut Vec<SystemId>,
    ) -> Result<(), ChangesetError> {
        let prefix = key_prefix(
            Keyspace::Forward,
            false,
            self.namespace(),
            Some(graph),
            Some(predicate),
        );
        let changed: Vec<SystemId> = self
            .write
            .request()
            .written_keys()
            .filter(|k| k.starts_with(&prefix))
            .filter_map(|k| match QuadKey::parse(k) {
                Ok((QuadKey::Forward { subject, .. }, None)) => Some(subject),
                _ => None,
            })
            .collect();
        held.retain(|id| !changed.contains(id));
        for subject in changed {
            for (object, _) in self.write.objects(graph, subject, predicate)? {
                if object.as_id().is_some() {
                    continue;
                }
                if let Term::Literal(held_literal) = self.decode(&object)?
                    && tokens.is_subset(&literal_tokens(declared, held_literal.as_ref())?)
                {
                    held.push(subject);
                    break;
                }
            }
        }
        Ok(())
    }

    /// The IRI or blank node `id` names, as a subject reads back.
    fn node_of(&mut self, id: SystemId) -> Result<NamedOrBlankNode, ChangesetError> {
        match self.decode(&EncodedTerm::id(id))? {
            Term::NamedNode(n) => Ok(n.into()),
            Term::BlankNode(b) => Ok(b.into()),
            Term::Literal(_) | Term::Triple(_) => Err(StoreError::corrupted(
                id.to_be_bytes().to_vec(),
                "a subject's ID names no IRI or blank node".to_string(),
            )
            .into()),
        }
    }
}
