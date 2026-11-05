/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use ultragraphdb_core_ids::{SystemId, TypePrefix};
use ultragraphdb_core_keys::{DictionaryEntry, Graph, QuadKey};
use ultragraphdb_core_mapping::MappingError;
use ultragraphdb_core_terms::{EncodedTerm, Hash128, SYSTEM_VOCABULARY};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store, VersionlessEntry};

use crate::errors::quad_error::QuadError;
use crate::types::quad_store::QuadStore;
use crate::types::term_origin::TermOrigin;
use crate::utils::dictionary_keys::{id_key, literal_key, term_key};
use crate::utils::skolem::is_skolem;

/// The terms one write names, resolved to their encodings: each IRI to its ID, got or created while the write runs
/// and shared with every other write of the IRI; each blank node to a new ID named by a fresh skolem IRI; each literal
/// to its encoding by content, the strings it hashes kept for the dictionary. The write's commit writes the dictionary
/// entries of every term it names that the store does not hold yet, those it created and the pending ones another
/// write created, so they are durable no later than the first commit naming them, whichever writes are refused. What
/// the write found absent is recorded by ID: an IRI it created, which no statement of its snapshot can hold.
#[derive(Debug, Default, Clone)]
pub struct WriteTerms {
    tenant: u64,
    /// Every IRI resolved, with its ID.
    ids: BTreeMap<String, SystemId>,
    /// The terms whose dictionary entries the write's commit writes, with their IDs: IRIs it created or took from a
    /// concurrent creator, its blank nodes' skolem IRIs, its statements' reifiers, the pending skolem IRIs and reifiers
    /// it found, and other partitions' entries it copies. Each but the copies is a pending entry the write holds until
    /// it is settled or released.
    written: BTreeMap<String, SystemId>,
    /// The blank nodes of the write by label, each with its ID.
    blanks: BTreeMap<String, SystemId>,
    /// The strings the write's literals name by hash.
    strings: BTreeMap<Hash128, String>,
    /// The IDs the write created, whose lists hold nothing.
    fresh: BTreeSet<SystemId>,
    /// The IRIs the write found absent and created, by ID.
    absent: BTreeSet<SystemId>,
    /// The terms, by ID, and the strings, by hash, of the dictionary entries decided but unapplied writes made, which
    /// no layer the write reads holds: read as the write's own are, never written again (7.5).
    inherited_ids: BTreeMap<SystemId, String>,
    inherited_strings: BTreeMap<Hash128, String>,
}

/// The base of the skolem IRIs of `tenant`'s blank nodes and reifiers: its default base, under the system
/// vocabulary, until an administrator sets one (the stage 4 design's open question).
pub fn tenant_base(tenant: u64) -> String {
    format!("{SYSTEM_VOCABULARY}tenant/{tenant}/")
}

impl WriteTerms {
    /// The terms of a write of `tenant`, none resolved yet.
    pub fn new(tenant: u64) -> Self {
        Self {
            tenant,
            ..Self::default()
        }
    }

    /// The tenant the write writes.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// Whether `id` was created by this write, so that no list keyed by it holds anything.
    pub fn is_fresh(&self, id: SystemId) -> bool {
        self.fresh.contains(&id)
    }

    /// Takes `iri` as known by `id` unless the write already resolved it: an IRI a decided but unapplied write
    /// created, whose dictionary entries that write's round writes before this one's, so this write neither creates
    /// it again nor writes its entries (7.5).
    pub fn know(&mut self, iri: &str, id: SystemId) {
        if !self.ids.contains_key(iri) {
            let _known: Option<SystemId> = self.ids.insert(iri.to_string(), id);
        }
    }

    /// Takes `iri` as known by `id`, an entity another partition owns that answered its registration with that ID, and
    /// copies its dictionary entries in both directions into this store's dictionary at the write's commit, so the
    /// store names it and finds its ID without asking its home.
    pub fn copy(&mut self, iri: &str, id: SystemId) {
        if !self.ids.contains_key(iri) {
            let _known: Option<SystemId> = self.ids.insert(iri.to_string(), id);
            let _copied: Option<SystemId> = self.written.insert(iri.to_string(), id);
        }
    }

    /// Whether the write resolved `iri` already: its own, taken from the dictionary, or copied from its home.
    pub fn knows(&self, iri: &str) -> bool {
        self.ids.contains_key(iri)
    }

    /// The ID of `iri`: the write's own, the dictionary's or a concurrent creator's, or a new one.
    ///
    /// # Errors
    ///
    /// [`QuadError::unknown_skolem`] for an IRI under the tenant's skolem base the dictionary does not hold; the
    /// refusals of creating an ID ([`QuadError::no_sequence`], [`QuadError::pending_full`], an IRI past a canonical
    /// key's length); the store's error.
    pub fn iri<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        iri: &str,
    ) -> Result<SystemId, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        if let Some(id) = self.ids.get(iri) {
            return Ok(*id);
        }
        let (id, origin) = quads
            .dictionary()
            .resolve(quads.store(), self.tenant, iri)?;
        let _resolved: Option<SystemId> = self.ids.insert(iri.to_string(), id);
        if origin != TermOrigin::Held {
            let _written: Option<SystemId> = self.written.insert(iri.to_string(), id);
        }
        if origin == TermOrigin::Created {
            let _fresh: bool = self.fresh.insert(id);
            let _absent: bool = self.absent.insert(id);
        }
        Ok(id)
    }

    /// The ID of the blank node labelled `label` in this write: one new ID per label, named by a skolem IRI drawn as
    /// [`QuadStore`]'s random source draws it.
    pub fn blank<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        label: &str,
    ) -> Result<SystemId, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        if let Some(id) = self.blanks.get(label) {
            return Ok(*id);
        }
        let (id, skolem) = quads.dictionary().mint_blank(quads.store(), self.tenant)?;
        let _resolved: Option<SystemId> = self.ids.insert(skolem.clone(), id);
        let _written: Option<SystemId> = self.written.insert(skolem, id);
        let _labelled: Option<SystemId> = self.blanks.insert(label.to_string(), id);
        let _fresh: bool = self.fresh.insert(id);
        Ok(id)
    }

    /// The ID of an IRI or blank-node subject.
    pub fn node<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        node: NamedOrBlankNodeRef<'_>,
    ) -> Result<SystemId, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        match node {
            NamedOrBlankNodeRef::NamedNode(n) => self.iri(quads, n.as_str()),
            NamedOrBlankNodeRef::BlankNode(b) => self.blank(quads, b.as_str()),
        }
    }

    /// The graph a graph name names: the default graph, or a named graph by its ID.
    pub fn graph<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        graph: GraphNameRef<'_>,
    ) -> Result<Graph, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        Ok(match graph {
            GraphNameRef::DefaultGraph => Graph::Default,
            GraphNameRef::NamedNode(n) => Graph::Named(self.iri(quads, n.as_str())?),
            GraphNameRef::BlankNode(b) => Graph::Named(self.blank(quads, b.as_str())?),
        })
    }

    /// The ID of a predicate.
    pub fn predicate<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        predicate: NamedNodeRef<'_>,
    ) -> Result<SystemId, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        self.iri(quads, predicate.as_str())
    }

    /// The encoding of an object, a literal's datatype IRI resolved as any IRI. A triple term is no object a write
    /// stores.
    pub fn term<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        term: TermRef<'_>,
    ) -> Result<EncodedTerm, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        match term {
            TermRef::NamedNode(n) => Ok(EncodedTerm::id(self.iri(quads, n.as_str())?)),
            TermRef::BlankNode(b) => Ok(EncodedTerm::id(self.blank(quads, b.as_str())?)),
            TermRef::Literal(literal) => {
                let encoded =
                    EncodedTerm::literal::<QuadError>(literal, |d| self.iri(quads, d.as_str()))?;
                for (hash, string) in encoded.strings() {
                    let _replaced: Option<String> = self.strings.insert(hash, string.to_string());
                }
                Ok(encoded.into_term())
            }
            TermRef::Triple(t) => Err(MappingError::unsupported_object(t.to_string()).into()),
        }
    }

    /// The encoding `term` has, as a delete or a read resolves it: an IRI as [`WriteTerms::iri`] resolves it; `None`
    /// for a blank node this write did not name and for an IRI under the skolem base the dictionary does not hold,
    /// which no statement can hold.
    pub fn find<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        term: TermRef<'_>,
    ) -> Result<Option<EncodedTerm>, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        match term {
            TermRef::NamedNode(n)
                if is_skolem(self.tenant, n.as_str()) && !self.ids.contains_key(n.as_str()) =>
            {
                let found = self.look_up(quads, n.as_str())?;
                Ok(found.map(EncodedTerm::id))
            }
            TermRef::NamedNode(n) => Ok(Some(EncodedTerm::id(self.iri(quads, n.as_str())?))),
            TermRef::BlankNode(b) => Ok(self.blanks.get(b.as_str()).map(|id| EncodedTerm::id(*id))),
            TermRef::Literal(literal) => {
                let encoded =
                    EncodedTerm::literal::<QuadError>(literal, |d| self.iri(quads, d.as_str()))?;
                Ok(Some(encoded.into_term()))
            }
            TermRef::Triple(t) => Err(MappingError::unsupported_object(t.to_string()).into()),
        }
    }

    /// A new statement ID of `subject`: a `Statement` ID in the subject's bucket, named by a new reifier, a skolem IRI
    /// of that bucket whose dictionary entries the write's commit writes.
    pub fn statement<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        subject: SystemId,
    ) -> Result<SystemId, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        let (id, reifier) =
            quads
                .dictionary()
                .mint_statement(quads.store(), self.tenant, subject)?;
        let _resolved: Option<SystemId> = self.ids.insert(reifier.clone(), id);
        let _written: Option<SystemId> = self.written.insert(reifier, id);
        let _fresh: bool = self.fresh.insert(id);
        Ok(id)
    }

    /// The statement ID the reifier `reifier` names, among the write's own reifiers or in the dictionary; `None` when
    /// it names none or names an ID that is no statement's.
    pub fn reifier<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        reifier: &str,
    ) -> Result<Option<SystemId>, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        let id = match self.ids.get(reifier) {
            Some(id) => Some(*id),
            None => self.look_up(quads, reifier)?,
        };
        Ok(id.filter(|id| id.type_prefix() == TypePrefix::STATEMENT))
    }

    /// The ID the dictionary has for `term`, pending or held, creating none: a pending one is taken as the write's own,
    /// its entries written by the write's commit.
    fn look_up<S, C, B>(
        &mut self,
        quads: &QuadStore<S, C, B>,
        term: &str,
    ) -> Result<Option<SystemId>, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        let found = quads
            .dictionary()
            .lookup(quads.store(), self.tenant, term)?;
        if let Some((id, TermOrigin::Pending)) = found {
            let _resolved: Option<SystemId> = self.ids.insert(term.to_string(), id);
            let _written: Option<SystemId> = self.written.insert(term.to_string(), id);
        }
        Ok(found.map(|(id, _)| id))
    }

    /// The dictionary entries the write's commit adds: both of each term in [`WriteTerms::written`], and each string
    /// its literals hash, which equal strings write equally.
    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let tenant = self.tenant;
        let terms = self.written.iter().flat_map(|(term, id)| {
            [
                (term_key(tenant, term), id.to_be_bytes().to_vec()),
                (id_key(tenant, *id), term.as_bytes().to_vec()),
            ]
        });
        let strings = self
            .strings
            .iter()
            .map(|(hash, s)| (literal_key(tenant, hash), s.as_bytes().to_vec()));
        terms.chain(strings).collect()
    }

    /// The string `hash` names among the write's own literals, whose dictionary entries its commit writes, or among
    /// the strings it inherited.
    pub fn pending_string(&self, hash: &Hash128) -> Option<&str> {
        self.strings
            .get(hash)
            .or_else(|| self.inherited_strings.get(hash))
            .map(String::as_str)
    }

    /// The text `id` names among the terms the write wrote or inherited, and whether it skolemizes a blank node.
    pub fn pending_iri(&self, id: SystemId) -> Option<(&str, bool)> {
        let text = self
            .written
            .iter()
            .find(|(_, i)| **i == id)
            .map(|(term, _)| term)
            .or_else(|| self.inherited_ids.get(&id))?;
        let blank = id.type_prefix() == TypePrefix::RESOURCE && is_skolem(self.tenant, text);
        Some((text.as_str(), blank))
    }

    /// Inherits the dictionary entries of `entries`, the entries without versions decided but unapplied writes made
    /// (7.5): each ID's term, known by its ID as [`WriteTerms::know`] takes it, and each string by its hash, so the
    /// write reads them as its own and writes neither again. Entries of another tenant or another kind, removals
    /// and entries that do not parse are left alone: the writes that made them parsed them.
    pub fn inherit(&mut self, entries: &[&VersionlessEntry]) {
        for entry in entries {
            let (Some(value), Ok((QuadKey::Dictionary { tenant, entry }, None))) =
                (entry.value(), QuadKey::parse(entry.key()))
            else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(value) else {
                continue;
            };
            match entry {
                DictionaryEntry::Id(id) if tenant == self.tenant => {
                    self.know(text, id);
                    let _inherited: Option<String> =
                        self.inherited_ids.insert(id, text.to_string());
                }
                DictionaryEntry::Literal(hash) if tenant == self.tenant => {
                    let _inherited: Option<String> =
                        self.inherited_strings.insert(hash, text.to_string());
                }
                DictionaryEntry::Id(_) | DictionaryEntry::Literal(_) | DictionaryEntry::Term(_) => {
                }
            }
        }
    }

    /// The IRIs the write found absent and created, by ID, in ID order: no statement of the write's snapshot names
    /// one, and a commit writing a statement of one since changes what the write found. No dictionary key is among
    /// them.
    pub fn absent(&self) -> impl Iterator<Item = SystemId> + '_ {
        self.absent.iter().copied()
    }

    /// The terms whose dictionary entries the write's commit writes, each with its ID, in term order.
    pub fn written(&self) -> impl Iterator<Item = (&str, SystemId)> {
        self.written.iter().map(|(term, id)| (term.as_str(), *id))
    }

    /// The terms of [`WriteTerms::written`] that `base` does not write, each with its ID, in term order: what a write
    /// continuing `base` on a clone of its terms took on its own.
    pub fn written_beyond<'t>(
        &'t self,
        base: &'t WriteTerms,
    ) -> impl Iterator<Item = (&'t str, SystemId)> {
        self.written()
            .filter(|(term, _)| !base.written.contains_key(*term))
    }
}
