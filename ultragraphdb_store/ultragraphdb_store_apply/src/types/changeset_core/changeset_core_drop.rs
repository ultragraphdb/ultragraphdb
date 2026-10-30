/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Drops, each one store drop under the apply lock: every version of every key under its prefixes removed physically,
//! then its entries written, finished at open when a crash interrupts it. A drop keeps the dictionary, so no UID is
//! reissued, and cancels a build of what it drops.

use std::collections::BTreeSet;
use std::ops::ControlFlow;

use oxrdf::vocab::rdf;
use oxrdf::{GraphNameRef, NamedNodeRef};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::{
    DEFAULT_GRAPH, Keyspace, QuadKey, ROOT_NAMESPACE, SPLIT_FLAG, key_prefix,
};
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_store_posting::{CachedList, List, PostingError};
use ultragraphdb_traits_store::{Entry, ListCache, Scan, Store, StoreSnapshot};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::drop_request::DropRequest;
use crate::types::drop_target::DropTarget;
use crate::types::executed::Executed;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::utils::lock::lock;
use crate::utils::quad_prefixes::predicate_prefixes;

/// The keyspaces of the quads, every one but the catalog's.
const QUAD_KEYSPACES: [Keyspace; 9] = [
    Keyspace::Forward,
    Keyspace::Reverse,
    Keyspace::SubjectPredicates,
    Keyspace::ObjectPredicates,
    Keyspace::ValueIndex,
    Keyspace::CountIndex,
    Keyspace::VectorIndex,
    Keyspace::Statements,
    Keyspace::Statistics,
];

impl<S, C> ChangesetCore<S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Drops `predicate` of `namespace` in every graph: its keys in every keyspace naming it, the records of the
    /// statement IDs its postings carry, and its catalog entries; and writes each subject- and object-predicate list
    /// that held it again without it, complete. The drop's timestamp.
    pub fn drop_predicate(
        &self,
        namespace: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        let lists = self.quads.lists();
        let Some(uid) = self.quads.uid_of(namespace, predicate.as_str())? else {
            return Ok(lists.applied_ts());
        };
        tracing::info!(predicate = predicate.as_str(), "dropping predicate");
        self.drop_predicate_uid(namespace, uid, None)
    }

    /// Drops predicate `uid` of `namespace` as [`ChangesetCore::drop_predicate`] does, under the apply lock the caller
    /// holds: at the next version, or at `at`, the version the commit step gave a drop decided in a round (7.7). The
    /// drop's version.
    pub(crate) fn drop_predicate_uid(
        &self,
        namespace: u64,
        uid: u64,
        at: Option<u64>,
    ) -> Result<u64, ChangesetError> {
        let lists = self.quads.lists();
        self.cancel(|r| r.namespace == namespace && r.predicate == uid);
        let latest = lists.applied_ts();
        lists.gate(latest)?;
        let snapshot = lists.store().snapshot(latest)?;
        let mut prefixes: BTreeSet<Vec<u8>> = BTreeSet::new();
        for p in predicate_prefixes(namespace, uid) {
            prefixes.extend(p.expand(&snapshot)?);
        }
        let mut entries = Vec::new();
        let forward = PredicatePrefix::new(Keyspace::Forward, false, namespace, uid, Vec::new());
        let reverse = PredicatePrefix::new(Keyspace::Reverse, false, namespace, uid, Vec::new());
        for graph in forward.graphs(&snapshot)? {
            for item in snapshot.scan(&Scan::prefix(forward.in_graph(graph))) {
                let key = item?.key().to_vec();
                let Ok((QuadKey::Forward { subject, .. }, None)) = QuadKey::parse(&key) else {
                    continue;
                };
                for (_, ids) in self.quads.objects(&key, latest)? {
                    for statement in ids {
                        prefixes.insert(
                            QuadKey::Statement {
                                namespace,
                                graph,
                                statement,
                            }
                            .to_bytes(),
                        );
                    }
                }
                let list = QuadKey::SubjectPredicates {
                    namespace,
                    graph,
                    subject,
                };
                self.rewrite_without(&list, uid, latest, &mut prefixes, &mut entries)?;
            }
        }
        for graph in reverse.graphs(&snapshot)? {
            for item in snapshot.scan(&Scan::prefix(reverse.in_graph(graph))) {
                let key = item?.key().to_vec();
                let Ok((QuadKey::Reverse { object, .. }, None)) = QuadKey::parse(&key) else {
                    continue;
                };
                let list = QuadKey::ObjectPredicates {
                    namespace,
                    graph,
                    object,
                };
                self.rewrite_without(&list, uid, latest, &mut prefixes, &mut entries)?;
            }
        }
        prefixes.insert(Catalog::predicate_key(namespace, uid));
        let prefixes: Vec<Vec<u8>> = prefixes.into_iter().collect();
        let ts = match at {
            Some(ts) => {
                lists.drop_prefixes_at(ts, &prefixes, entries)?;
                ts
            }
            None => lists.drop_prefixes_now(&prefixes, entries)?,
        };
        self.forget_catalogs();
        Ok(ts)
    }

    /// The drop `request` asks in `namespace` as a proposal of the commit step (7.7): an execution at the watermark that
    /// writes nothing but the key ranges of what it drops, which the commit step holds as a write of those ranges at
    /// its version, and which `apply_round` drops once its round's batch is written. A predicate's ranges are its keys
    /// in each graph holding it and its catalog entry; a graph's, its keys in every quad keyspace; a namespace's data,
    /// every quad keyspace of it; a namespace, its catalog too; every namespace, every key of every quad keyspace and
    /// catalog. A predicate or a graph the store does not know, or a blank-node graph, is dropped as nothing.
    pub fn prepare_drop(
        &self,
        namespace: u64,
        request: DropRequest<'_>,
    ) -> Result<Executed, ChangesetError> {
        let lists = self.quads.lists();
        let read_ts = lists.applied_ts();
        let catalog = (*self.catalog(namespace)?).clone();
        let (target, prefixes) = match request {
            DropRequest::Predicate(predicate) => {
                match self.quads.uid_of(namespace, predicate.as_str())? {
                    Some(uid) => {
                        lists.gate(read_ts)?;
                        let snapshot = lists.store().snapshot(read_ts)?;
                        let mut prefixes = Vec::new();
                        for p in predicate_prefixes(namespace, uid) {
                            prefixes.extend(p.expand(&snapshot)?);
                        }
                        prefixes.push(Catalog::predicate_key(namespace, uid));
                        (Some(DropTarget::Predicate(uid)), prefixes)
                    }
                    None => (None, Vec::new()),
                }
            }
            DropRequest::Graph(graph) => match self.graph_uid(namespace, graph)? {
                Some(uid) => (Some(DropTarget::Graph(uid)), graph_prefixes(namespace, uid)),
                None => (None, Vec::new()),
            },
            DropRequest::Data => (Some(DropTarget::Data), namespace_prefixes(namespace, false)),
            DropRequest::Namespace => (
                Some(DropTarget::Namespace),
                namespace_prefixes(namespace, true),
            ),
            DropRequest::All => (Some(DropTarget::All), every_prefix()),
        };
        let ranges = prefixes
            .into_iter()
            .map(|start| {
                let end = successor(&start);
                (start, end)
            })
            .collect();
        Ok(Executed::dropping(
            namespace, read_ts, catalog, target, ranges,
        ))
    }

    /// Drops `target` of `namespace` at `version`, the version the commit step gave the drop decided in a round, under
    /// the apply lock the caller holds (7.7).
    pub(crate) fn drop_at(
        &self,
        namespace: u64,
        target: DropTarget,
        version: u64,
    ) -> Result<u64, ChangesetError> {
        let at = Some(version);
        match target {
            DropTarget::Predicate(uid) => self.drop_predicate_uid(namespace, uid, at),
            DropTarget::Graph(uid) => self.drop_graph_uid(namespace, uid, at),
            DropTarget::Data => self.drop_data_of(namespace, at),
            DropTarget::Namespace => self.drop_namespace_of(namespace, at),
            DropTarget::All => self.drop_every(at),
        }
    }

    /// `prefixes` dropped with `entries` written after: at `at`, the version the commit step gave a drop decided in a
    /// round, or at the next version. The drop's version.
    fn drop_prefixes(
        &self,
        prefixes: &[Vec<u8>],
        entries: Vec<Entry>,
        at: Option<u64>,
    ) -> Result<u64, ChangesetError> {
        let lists = self.quads.lists();
        Ok(match at {
            Some(ts) => {
                lists.drop_prefixes_at(ts, prefixes, entries)?;
                ts
            }
            None => lists.drop_prefixes_now(prefixes, entries)?,
        })
    }

    /// The UID of `graph` in `namespace`: the default graph's, a named graph's when the store knows it; `None` for a
    /// graph it does not know and for a blank node, which names no stored graph.
    fn graph_uid(
        &self,
        namespace: u64,
        graph: GraphNameRef<'_>,
    ) -> Result<Option<u64>, ChangesetError> {
        Ok(match graph {
            GraphNameRef::DefaultGraph => Some(DEFAULT_GRAPH),
            GraphNameRef::NamedNode(n) => self.quads.uid_of(namespace, n.as_str())?,
            GraphNameRef::BlankNode(_) => None,
        })
    }

    /// Adds the predicate list `list`, its split parts with it, to the drop's prefixes, and its postings but
    /// `predicate`'s, complete at `version`, to the drop's entries.
    fn rewrite_without(
        &self,
        list: &QuadKey,
        predicate: u64,
        version: u64,
        prefixes: &mut BTreeSet<Vec<u8>>,
        entries: &mut Vec<Entry>,
    ) -> Result<(), ChangesetError> {
        let key = list.to_bytes();
        if prefixes.contains(&key) {
            return Ok(());
        }
        let lists = self.quads.lists();
        let held = lists.read(&key, version)?;
        let mut kept = List::empty(key.clone());
        kept.set_ts(version);
        held.iterate(version, 0, |p| {
            if p.uid() != predicate {
                kept.add_posting(p.clone(), false);
            }
            Ok::<_, PostingError>(ControlFlow::Continue(()))
        })?;
        kept.commit_mutation(version);
        if let Some(out) = kept.rollup(u64::MAX, true, lists.config().max_list_size())? {
            entries.extend(
                out.entries(&key, &[], version)?
                    .into_iter()
                    .filter(|e| !e.is_deletion()),
            );
        }
        let mut parts = key.clone();
        parts[0] |= SPLIT_FLAG;
        prefixes.insert(key);
        prefixes.insert(parts);
        Ok(())
    }

    /// Drops `graph` of `namespace`: every key of the graph. The drop's timestamp.
    pub fn drop_graph(
        &self,
        namespace: u64,
        graph: GraphNameRef<'_>,
    ) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        let Some(uid) = self.graph_uid(namespace, graph)? else {
            return Ok(self.quads.lists().applied_ts());
        };
        self.drop_graph_uid(namespace, uid, None)
    }

    /// Drops graph `uid` of `namespace` under the apply lock the caller holds, at `at` or the next version.
    fn drop_graph_uid(
        &self,
        namespace: u64,
        uid: u64,
        at: Option<u64>,
    ) -> Result<u64, ChangesetError> {
        self.drop_prefixes(&graph_prefixes(namespace, uid), Vec::new(), at)
    }

    /// Drops the data of `namespace`, every key of every quad keyspace, and keeps its catalog. A build in the namespace
    /// is cancelled, its predicate declared as the change asked, since no data is left to build from. The drop's
    /// timestamp.
    pub fn drop_data(&self, namespace: u64) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        self.drop_data_of(namespace, None)
    }

    /// Drops the data of `namespace` as [`ChangesetCore::drop_data`] does, under the apply lock the caller holds, at
    /// `at` or the next version.
    fn drop_data_of(&self, namespace: u64, at: Option<u64>) -> Result<u64, ChangesetError> {
        let entries = match self.cancel(|r| r.namespace == namespace) {
            Some(r) => vec![Catalog::entry(
                namespace,
                r.predicate,
                &Declaration::Predicate(r.new),
                self.quads.lists().applied_ts(),
            )],
            None => Vec::new(),
        };
        let ts = self.drop_prefixes(&namespace_prefixes(namespace, false), entries, at)?;
        self.forget_catalogs();
        Ok(ts)
    }

    /// Drops everything `namespace` holds, its catalog included, and writes its initial catalog at timestamp 1 when
    /// its `rdf:type` has a UID; a namespace whose has none was never written, and its first write writes the
    /// catalog. The drop's timestamp.
    pub fn drop_namespace(&self, namespace: u64) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        self.drop_namespace_of(namespace, None)
    }

    /// Drops `namespace` as [`ChangesetCore::drop_namespace`] does, under the apply lock the caller holds, at `at` or
    /// the next version.
    fn drop_namespace_of(&self, namespace: u64, at: Option<u64>) -> Result<u64, ChangesetError> {
        self.cancel(|r| r.namespace == namespace);
        let entries = self.initial_entries(namespace)?;
        let ts = self.drop_prefixes(&namespace_prefixes(namespace, true), entries, at)?;
        self.forget_catalogs();
        Ok(ts)
    }

    /// Drops every quad key of every namespace, the catalogs included, and writes the root namespace's initial catalog
    /// at timestamp 1. The dictionary is kept. The drop's timestamp.
    pub fn drop_all(&self) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        self.drop_every(None)
    }

    /// Drops every namespace as [`ChangesetCore::drop_all`] does, under the apply lock the caller holds, at `at` or the
    /// next version.
    fn drop_every(&self, at: Option<u64>) -> Result<u64, ChangesetError> {
        self.cancel(|_| true);
        let entries = self.initial_entries(ROOT_NAMESPACE)?;
        let ts = self.drop_prefixes(&every_prefix(), entries, at)?;
        self.forget_catalogs();
        Ok(ts)
    }

    /// The entries of `namespace`'s initial catalog at timestamp 1, none when its `rdf:type` has no UID.
    fn initial_entries(&self, namespace: u64) -> Result<Vec<Entry>, ChangesetError> {
        Ok(match self.quads.uid_of(namespace, rdf::TYPE.as_str())? {
            Some(rdf_type) => Catalog::initial(namespace, rdf_type).entries(1),
            None => Vec::new(),
        })
    }
}

/// The prefixes `prefix` gives for `keyspace`'s keys and, when its lists split, their split parts.
fn parts(keyspace: Keyspace, prefix: impl Fn(bool) -> Vec<u8>) -> Vec<Vec<u8>> {
    if keyspace.is_list() {
        vec![prefix(false), prefix(true)]
    } else {
        vec![prefix(false)]
    }
}

/// Every quad keyspace's prefix of graph `graph` of `namespace`.
fn graph_prefixes(namespace: u64, graph: u64) -> Vec<Vec<u8>> {
    QUAD_KEYSPACES
        .iter()
        .flat_map(|k| {
            parts(*k, |split| {
                key_prefix(*k, split, namespace, Some(graph), None)
            })
        })
        .collect()
}

/// Every keyspace's prefix, the catalog's and every quad keyspace's, of every namespace.
fn every_prefix() -> Vec<Vec<u8>> {
    std::iter::once(Keyspace::Catalog)
        .chain(QUAD_KEYSPACES)
        .flat_map(|k| {
            parts(k, |split| {
                vec![k.byte() | if split { SPLIT_FLAG } else { 0 }]
            })
        })
        .collect()
}

/// Every quad keyspace's prefix of `namespace`, and its catalog's when `catalog` holds.
fn namespace_prefixes(namespace: u64, catalog: bool) -> Vec<Vec<u8>> {
    let keyspaces = QUAD_KEYSPACES
        .iter()
        .copied()
        .chain(catalog.then_some(Keyspace::Catalog));
    keyspaces
        .flat_map(|k| parts(k, |split| key_prefix(k, split, namespace, None, None)))
        .collect()
}

/// The first key past every key `prefix` begins: the prefix with its last byte below 0xff raised by one and what follows
/// it removed; empty, past the last key, when every byte is 0xff.
fn successor(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.pop() {
        if last < u8::MAX {
            end.push(last + 1);
            return end;
        }
    }
    end
}
