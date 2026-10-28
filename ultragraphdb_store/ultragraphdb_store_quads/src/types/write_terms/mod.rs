/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use ultragraphdb_core_keys::DEFAULT_GRAPH;
use ultragraphdb_core_terms::{EncodedTerm, Hash128, SYSTEM_VOCABULARY};
use ultragraphdb_traits_store::{Store, VersionlessEntry};

use crate::errors::quad_error::{QuadError, QuadErrorEnum};
use crate::types::leases::Leases;
use crate::utils::dictionary_keys::{
    HASH_TO_STRING, UID_TO_IRI, iri_key, string_key, uid_key, uid_value,
};

/// The terms one write names, resolved to their encodings: each IRI to the UID the dictionary holds for it, or to a
/// UID drawn from the leases when it is new to the store; each blank node to a fresh UID, its skolem IRI new; each
/// literal to its encoding by content, the strings it hashes kept for the dictionary. A new IRI's UID is confirmed at
/// commit, when its dictionary entries are written, or the write is refused if a commit since claimed the IRI.
#[derive(Debug, Default, Clone)]
pub struct WriteTerms {
    namespace: u64,
    /// The IRIs new to the store, with their UIDs and whether each skolemizes a blank node.
    created: BTreeMap<String, (u64, bool)>,
    /// The IRIs the dictionary already held, with their UIDs.
    known: BTreeMap<String, u64>,
    /// The blank nodes of the write by label, each with its UID.
    blanks: BTreeMap<String, u64>,
    /// The strings the write's literals name by hash.
    strings: BTreeMap<Hash128, String>,
    /// The UIDs the write created, whose lists hold nothing.
    fresh: BTreeSet<u64>,
    /// The dictionary keys the write looked up and found absent.
    absent: BTreeSet<Vec<u8>>,
    /// The IRIs, by UID, and the strings, by hash, of the dictionary entries decided but unapplied writes made, which
    /// no layer the write reads holds: read as the write's own are, never written again (7.5).
    inherited_iris: BTreeMap<u64, (String, bool)>,
    inherited_strings: BTreeMap<Hash128, String>,
}

/// The base of the skolem IRIs of `namespace`'s blank nodes: its default base, under the system vocabulary, until an
/// administrator sets one (the stage 4 design's open question).
pub fn namespace_base(namespace: u64) -> String {
    format!("{SYSTEM_VOCABULARY}namespace/{namespace}/")
}

impl WriteTerms {
    pub fn new(namespace: u64) -> Self {
        Self {
            namespace,
            ..Self::default()
        }
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// Whether `uid` was created by this write, so that no list keyed by it holds anything.
    pub fn is_fresh(&self, uid: u64) -> bool {
        self.fresh.contains(&uid)
    }

    /// Takes `iri` as known by `uid` unless the write already resolved it: an IRI a decided but unapplied write
    /// created, whose dictionary entry that write's round writes before this one's, so this write neither creates it
    /// again nor writes its entry (7.5).
    pub fn know(&mut self, iri: &str, uid: u64) {
        if !self.known.contains_key(iri) && !self.created.contains_key(iri) {
            self.known.insert(iri.to_string(), uid);
        }
    }

    /// The UID of `iri`: the dictionary's, or a new one.
    pub fn iri<S: Store>(
        &mut self,
        store: &S,
        leases: &Leases,
        iri: &str,
    ) -> Result<u64, QuadError> {
        if let Some(uid) = self
            .known
            .get(iri)
            .or(self.created.get(iri).map(|(u, _)| u))
        {
            return Ok(*uid);
        }
        let key = iri_key(self.namespace, iri);
        if let Some(value) = store.get_versionless(&key)? {
            let uid = decode_uid(&key, &value)?;
            self.known.insert(iri.to_string(), uid);
            return Ok(uid);
        }
        self.absent.insert(key);
        let uid = leases.next_uid(store, self.namespace)?;
        self.created.insert(iri.to_string(), (uid, false));
        self.fresh.insert(uid);
        Ok(uid)
    }

    /// The UID `iri` has, in the dictionary or among the IRIs this write created, allocating none: what a delete
    /// resolves its terms with, so that a delete of a term nothing holds writes no dictionary entry.
    pub fn find_iri<S: Store>(&mut self, store: &S, iri: &str) -> Result<Option<u64>, QuadError> {
        if let Some(uid) = self
            .known
            .get(iri)
            .or(self.created.get(iri).map(|(u, _)| u))
        {
            return Ok(Some(*uid));
        }
        let key = iri_key(self.namespace, iri);
        let Some(value) = store.get_versionless(&key)? else {
            self.absent.insert(key);
            return Ok(None);
        };
        let uid = decode_uid(&key, &value)?;
        self.known.insert(iri.to_string(), uid);
        Ok(Some(uid))
    }

    /// The encoding `term` has, allocating no UID: `None` when it names an IRI, a blank node or a datatype the store
    /// does not hold, which no statement can then hold either. A blank node is found only among this write's own.
    pub fn find<S: Store>(
        &mut self,
        store: &S,
        term: TermRef<'_>,
    ) -> Result<Option<EncodedTerm>, QuadError> {
        match term {
            TermRef::NamedNode(n) => Ok(self.find_iri(store, n.as_str())?.map(EncodedTerm::uid)),
            TermRef::BlankNode(b) => Ok(self.blanks.get(b.as_str()).map(|u| EncodedTerm::uid(*u))),
            TermRef::Literal(literal) => {
                let mut missing = false;
                let encoded = EncodedTerm::literal::<QuadError>(literal, |d| {
                    Ok(self.find_iri(store, d.as_str())?.unwrap_or_else(|| {
                        missing = true;
                        0
                    }))
                })?;
                Ok((!missing).then(|| encoded.into_term()))
            }
            TermRef::Triple(t) => Err(QuadError(QuadErrorEnum::Mapping(
                ultragraphdb_core_mapping::MappingError::UnsupportedObject(t.to_string()),
            ))),
        }
    }

    /// The UID of the blank node labelled `label` in this write: one fresh UID per label, its skolem IRI under the
    /// namespace's base.
    pub fn blank<S: Store>(
        &mut self,
        store: &S,
        leases: &Leases,
        label: &str,
    ) -> Result<u64, QuadError> {
        if let Some(uid) = self.blanks.get(label) {
            return Ok(*uid);
        }
        let uid = leases.next_uid(store, self.namespace)?;
        let skolem = format!("{}.well-known/genid/{uid}", namespace_base(self.namespace));
        self.created.insert(skolem, (uid, true));
        self.blanks.insert(label.to_string(), uid);
        self.fresh.insert(uid);
        Ok(uid)
    }

    /// The UID of an IRI or blank-node subject.
    pub fn node<S: Store>(
        &mut self,
        store: &S,
        leases: &Leases,
        node: NamedOrBlankNodeRef<'_>,
    ) -> Result<u64, QuadError> {
        match node {
            NamedOrBlankNodeRef::NamedNode(n) => self.iri(store, leases, n.as_str()),
            NamedOrBlankNodeRef::BlankNode(b) => self.blank(store, leases, b.as_str()),
        }
    }

    /// The UID of a graph, 0 for the default graph.
    pub fn graph<S: Store>(
        &mut self,
        store: &S,
        leases: &Leases,
        graph: GraphNameRef<'_>,
    ) -> Result<u64, QuadError> {
        match graph {
            GraphNameRef::DefaultGraph => Ok(DEFAULT_GRAPH),
            GraphNameRef::NamedNode(n) => self.iri(store, leases, n.as_str()),
            GraphNameRef::BlankNode(b) => self.blank(store, leases, b.as_str()),
        }
    }

    pub fn predicate<S: Store>(
        &mut self,
        store: &S,
        leases: &Leases,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, QuadError> {
        self.iri(store, leases, predicate.as_str())
    }

    /// The encoding of an object. A triple term is no object a write stores.
    pub fn term<S: Store>(
        &mut self,
        store: &S,
        leases: &Leases,
        term: TermRef<'_>,
    ) -> Result<EncodedTerm, QuadError> {
        match term {
            TermRef::NamedNode(n) => Ok(EncodedTerm::uid(self.iri(store, leases, n.as_str())?)),
            TermRef::BlankNode(b) => Ok(EncodedTerm::uid(self.blank(store, leases, b.as_str())?)),
            TermRef::Literal(literal) => {
                let encoded = EncodedTerm::literal::<QuadError>(literal, |d| {
                    self.iri(store, leases, d.as_str())
                })?;
                for (hash, string) in encoded.strings() {
                    self.strings.insert(hash, string.to_string());
                }
                Ok(encoded.into_term())
            }
            TermRef::Triple(t) => Err(QuadError(QuadErrorEnum::Mapping(
                ultragraphdb_core_mapping::MappingError::UnsupportedObject(t.to_string()),
            ))),
        }
    }

    /// The dictionary entries the write's commit adds: each new IRI's two, and each string its literals hash, which
    /// equal strings write equally.
    pub fn entries(&self) -> Vec<(Vec<u8>, Vec<u8>)> {
        let ns = self.namespace;
        let iris = self.created.iter().flat_map(|(iri, (uid, skolem))| {
            [
                (iri_key(ns, iri), uid.to_be_bytes().to_vec()),
                (uid_key(ns, *uid), uid_value(iri, *skolem)),
            ]
        });
        let strings = self
            .strings
            .iter()
            .map(|(hash, s)| (string_key(ns, hash), s.as_bytes().to_vec()));
        iris.chain(strings).collect()
    }

    /// The string `hash` names among the write's own literals, whose dictionary entries its commit writes, or among
    /// the strings it inherited.
    pub fn pending_string(&self, hash: &Hash128) -> Option<&str> {
        self.strings
            .get(hash)
            .or_else(|| self.inherited_strings.get(hash))
            .map(String::as_str)
    }

    /// The IRI `uid` names among the IRIs the write created or inherited, and whether it skolemizes a blank node.
    pub fn pending_iri(&self, uid: u64) -> Option<(&str, bool)> {
        self.created
            .iter()
            .find(|(_, (u, _))| *u == uid)
            .map(|(iri, (_, skolem))| (iri.as_str(), *skolem))
            .or_else(|| {
                self.inherited_iris
                    .get(&uid)
                    .map(|(iri, skolem)| (iri.as_str(), *skolem))
            })
    }

    /// Inherits the dictionary entries of `entries`, the entries without versions decided but unapplied writes made
    /// (7.5): each UID's IRI, known by its UID as [`WriteTerms::know`] takes it, and each string by its hash, so the
    /// write reads them as its own and writes neither again. Entries of another namespace, other tags, removals and
    /// entries that do not parse are left alone: the writes that made them parsed them.
    pub fn inherit(&mut self, entries: &[&VersionlessEntry]) {
        let prefix = self.namespace.to_be_bytes();
        for entry in entries {
            let (key, Some(value)) = (entry.key(), entry.value()) else {
                continue;
            };
            let Some(rest) = key.strip_prefix(prefix.as_slice()) else {
                continue;
            };
            match rest.split_first() {
                Some((&UID_TO_IRI, uid)) => {
                    let (Ok(uid), Some((flag, iri))) =
                        (<[u8; 8]>::try_from(uid), value.split_first())
                    else {
                        continue;
                    };
                    let Ok(iri) = std::str::from_utf8(iri) else {
                        continue;
                    };
                    let uid = u64::from_be_bytes(uid);
                    self.know(iri, uid);
                    self.inherited_iris
                        .insert(uid, (iri.to_string(), *flag == 1));
                }
                Some((&HASH_TO_STRING, hash)) => {
                    let (Ok(hash), Ok(string)) =
                        (<[u8; 16]>::try_from(hash), std::str::from_utf8(value))
                    else {
                        continue;
                    };
                    self.inherited_strings
                        .insert(Hash128::from_bytes(hash), string.to_string());
                }
                _ => {}
            }
        }
    }

    /// The dictionary keys the write looked up and found absent, in key order: an IRI it created, and one a delete
    /// looked for and the store did not hold. A commit creating one of these terms after the write's read changes
    /// what the write found.
    pub fn absent(&self) -> impl Iterator<Item = &[u8]> {
        self.absent.iter().map(Vec::as_slice)
    }

    /// The IRIs new to the store, with their UIDs.
    pub fn created(&self) -> impl Iterator<Item = (&str, u64)> {
        self.created
            .iter()
            .map(|(iri, (uid, _))| (iri.as_str(), *uid))
    }
}

/// The UID an IRI's dictionary entry holds.
pub(crate) fn decode_uid(key: &[u8], value: &[u8]) -> Result<u64, QuadError> {
    let raw: [u8; 8] = value
        .try_into()
        .map_err(|_| QuadError::Corrupted(key, "an IRI's UID is not 8 bytes"))?;
    Ok(u64::from_be_bytes(raw))
}
