/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Drops, each one store drop under the apply lock: every version of every key under its prefixes removed physically,
//! then its entries written, finished at open when a crash interrupts it. A drop keeps the dictionary, so no UID is
//! reissued, and cancels a build of what it drops.

use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::sync::Arc;

use oxrdf::vocab::rdf;
use oxrdf::{GraphNameRef, NamedNodeRef};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Graph, Keyspace, QuadKey, SPLIT_FLAG, key_prefix};
use ultragraphdb_store_posting::{CachedList, List, PostingError};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Entry, ListCache, Scan, Store, StoreSnapshot};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_core::ChangesetCore;
use crate::types::drop_request::DropRequest;
use crate::types::drop_target::DropTarget;
use crate::types::executed::Executed;
use crate::types::predicate_prefix::PredicatePrefix;
use crate::types::rebuild::Rebuild;
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

/// The keyspaces of the registrations between partitions, which name no graph: dropped with a tenant's data, never with
/// a graph.
const REGISTRATION_KEYSPACES: [Keyspace; 2] = [Keyspace::Marks, Keyspace::Referrers];

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// Drops `predicate` of `tenant` in every graph: its keys in every keyspace naming it, the records of the
    /// statement IDs its postings carry, and its catalog entries; and writes each subject- and object-predicate list
    /// that held it again without it, complete. The drop's timestamp.
    pub fn drop_predicate(
        &self,
        tenant: u64,
        predicate: NamedNodeRef<'_>,
    ) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        let lists = self.quads.lists();
        let Some(uid) = self.quads.id_of(tenant, predicate.as_str())? else {
            return Ok(lists.applied_ts());
        };
        tracing::info!(predicate = predicate.as_str(), "dropping predicate");
        self.drop_predicate_uid(tenant, uid, None)
    }

    /// Drops predicate `uid` of `tenant` as [`ChangesetCore::drop_predicate`] does, under the apply lock the caller
    /// holds: at the next version, or at `at`, the version the commit step gave a drop decided in a round (7.7). The
    /// drop's version.
    pub(crate) fn drop_predicate_uid(
        &self,
        tenant: u64,
        uid: SystemId,
        at: Option<u64>,
    ) -> Result<u64, ChangesetError> {
        let lists = self.quads.lists();
        let _cancelled: Option<Rebuild> = self.cancel(|r| r.tenant == tenant && r.predicate == uid);
        let latest = lists.applied_ts();
        lists.gate(latest)?;
        let snapshot = lists.store().snapshot(latest)?;
        let mut prefixes: BTreeSet<Vec<u8>> = BTreeSet::new();
        for p in predicate_prefixes(tenant, uid) {
            prefixes.extend(p.expand(&snapshot)?);
        }
        let mut entries = Vec::new();
        let forward = PredicatePrefix::new(Keyspace::Forward, false, tenant, uid, Vec::new());
        let reverse = PredicatePrefix::new(Keyspace::Reverse, false, tenant, uid, Vec::new());
        for graph in forward.graphs(&snapshot)? {
            for item in snapshot.scan(&Scan::prefix(forward.in_graph(graph))) {
                let key = item?.key().to_vec();
                let Ok((QuadKey::Forward { subject, .. }, None)) = QuadKey::parse(&key) else {
                    continue;
                };
                for (_, ids) in self.quads.objects(&key, latest)? {
                    for statement in ids {
                        let _inserted: bool = prefixes.insert(
                            QuadKey::Statement {
                                tenant,
                                graph,
                                statement,
                            }
                            .to_bytes(),
                        );
                    }
                }
                let list = QuadKey::SubjectPredicates {
                    tenant,
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
                    tenant,
                    graph,
                    object,
                };
                self.rewrite_without(&list, uid, latest, &mut prefixes, &mut entries)?;
            }
        }
        let prefixes: Vec<Vec<u8>> = prefixes.into_iter().collect();
        let ts = match at {
            Some(ts) => {
                lists.drop_prefixes_at(ts, &prefixes, entries)?;
                ts
            }
            None => lists.drop_prefixes_now(&prefixes, entries)?,
        };
        // The predicate's declaration leaves the catalog held in memory, so no index of it is maintained until the
        // metadata group's version removing it takes effect.
        self.set_catalog(tenant, |catalog| {
            let _declared: bool = catalog.remove_predicate(uid);
            Ok(())
        })?;
        Ok(ts)
    }

    /// The drop `request` asks in `tenant` as a proposal of the commit step (7.7): an execution at the watermark that
    /// writes nothing but the key ranges of what it drops, which the commit step holds as a write of those ranges at
    /// its version, and which `apply_round` drops once its round's batch is written. A predicate's ranges are its keys
    /// in each graph holding it and its catalog entry; a graph's, its keys in every quad keyspace; a tenant's data,
    /// every quad keyspace of it; a tenant, its catalog too. A predicate or a graph the store does not know, or a
    /// blank-node graph, is dropped as nothing.
    pub fn prepare_drop(
        &self,
        tenant: u64,
        request: DropRequest<'_>,
    ) -> Result<Executed, ChangesetError> {
        let lists = self.quads.lists();
        let read_ts = lists.applied_ts();
        let catalog = (*self.catalog(tenant)?).clone();
        let (target, prefixes) = match request {
            DropRequest::Predicate(predicate) => {
                match self.quads.id_of(tenant, predicate.as_str())? {
                    Some(uid) => {
                        lists.gate(read_ts)?;
                        let snapshot = lists.store().snapshot(read_ts)?;
                        let mut prefixes = Vec::new();
                        for p in predicate_prefixes(tenant, uid) {
                            prefixes.extend(p.expand(&snapshot)?);
                        }
                        (Some(DropTarget::Predicate(uid)), prefixes)
                    }
                    None => (None, Vec::new()),
                }
            }
            DropRequest::Graph(graph) => match self.graph_uid(tenant, graph)? {
                Some(uid) => (Some(DropTarget::Graph(uid)), graph_prefixes(tenant, uid)),
                None => (None, Vec::new()),
            },
            DropRequest::Data => (Some(DropTarget::Data), tenant_prefixes(tenant)),
            DropRequest::Tenant => (Some(DropTarget::Tenant), tenant_prefixes(tenant)),
        };
        let ranges = prefixes
            .into_iter()
            .map(|start| {
                let end = successor(&start);
                (start, end)
            })
            .collect();
        Ok(Executed::dropping(tenant, read_ts, catalog, target, ranges))
    }

    /// Drops `target` of `tenant` at `version`, the version the commit step gave the drop decided in a round, under
    /// the apply lock the caller holds (7.7).
    pub(crate) fn drop_at(
        &self,
        tenant: u64,
        target: DropTarget,
        version: u64,
    ) -> Result<u64, ChangesetError> {
        let at = Some(version);
        match target {
            DropTarget::Predicate(uid) => self.drop_predicate_uid(tenant, uid, at),
            DropTarget::Graph(uid) => self.drop_graph_uid(tenant, uid, at),
            DropTarget::Data => self.drop_data_of(tenant, at),
            DropTarget::Tenant => self.drop_tenant_of(tenant, at),
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

    /// The graph `graph` names in `tenant`: the default graph, a named graph when the store knows it; `None` for a
    /// graph it does not know and for a blank node, which names no stored graph.
    fn graph_uid(
        &self,
        tenant: u64,
        graph: GraphNameRef<'_>,
    ) -> Result<Option<Graph>, ChangesetError> {
        Ok(match graph {
            GraphNameRef::DefaultGraph => Some(DEFAULT_GRAPH),
            GraphNameRef::NamedNode(n) => self.quads.id_of(tenant, n.as_str())?.map(Graph::Named),
            GraphNameRef::BlankNode(_) => None,
        })
    }

    /// Adds the predicate list `list`, its split parts with it, to the drop's prefixes, and its postings but
    /// `predicate`'s, complete at `version`, to the drop's entries.
    fn rewrite_without(
        &self,
        list: &QuadKey,
        predicate: SystemId,
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
            if p.uid() != predicate.to_u128() {
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
        if let Some(first) = parts.first_mut() {
            *first |= SPLIT_FLAG;
        }
        let _new_key: bool = prefixes.insert(key);
        let _new_parts: bool = prefixes.insert(parts);
        Ok(())
    }

    /// Drops `graph` of `tenant`: every key of the graph. The drop's timestamp.
    pub fn drop_graph(&self, tenant: u64, graph: GraphNameRef<'_>) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        let Some(uid) = self.graph_uid(tenant, graph)? else {
            return Ok(self.quads.lists().applied_ts());
        };
        self.drop_graph_uid(tenant, uid, None)
    }

    /// Drops `graph` of `tenant` under the apply lock the caller holds, at `at` or the next version.
    fn drop_graph_uid(
        &self,
        tenant: u64,
        graph: Graph,
        at: Option<u64>,
    ) -> Result<u64, ChangesetError> {
        self.drop_prefixes(&graph_prefixes(tenant, graph), Vec::new(), at)
    }

    /// Drops the data of `tenant`, every key of every quad keyspace, and keeps its catalog. A build in the tenant
    /// is cancelled, its predicate declared as the change asked, since no data is left to build from. The drop's
    /// timestamp.
    pub fn drop_data(&self, tenant: u64) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        self.drop_data_of(tenant, None)
    }

    /// Drops the data of `tenant` as [`ChangesetCore::drop_data`] does, under the apply lock the caller holds, at
    /// `at` or the next version.
    fn drop_data_of(&self, tenant: u64, at: Option<u64>) -> Result<u64, ChangesetError> {
        let cancelled = self.cancel(|r| r.tenant == tenant);
        let ts = self.drop_prefixes(&tenant_prefixes(tenant), Vec::new(), at)?;
        self.quads.forget_referrers(tenant);
        if let Some(r) = cancelled {
            self.set_catalog(tenant, |catalog| {
                catalog.declare_predicate(r.predicate, r.new)?;
                Ok(())
            })?;
        }
        Ok(ts)
    }

    /// Drops everything `tenant` holds and holds its initial catalog in memory: `rdf:type` declared an edge when it
    /// has a UID; a tenant whose has none was never written, and its first write gives it the initial catalog. The
    /// metadata group commits the tenant's initial catalog as a version of its own. The drop's timestamp.
    pub fn drop_tenant(&self, tenant: u64) -> Result<u64, ChangesetError> {
        let _apply = lock(&self.apply);
        self.drop_tenant_of(tenant, None)
    }

    /// Drops `tenant` as [`ChangesetCore::drop_tenant`] does, under the apply lock the caller holds, at `at` or
    /// the next version.
    fn drop_tenant_of(&self, tenant: u64, at: Option<u64>) -> Result<u64, ChangesetError> {
        let _cancelled: Option<Rebuild> = self.cancel(|r| r.tenant == tenant);
        let initial = self.initial_catalog(tenant)?;
        let ts = self.drop_prefixes(&tenant_prefixes(tenant), Vec::new(), at)?;
        self.quads.forget_referrers(tenant);
        let _dropped: Option<Arc<Catalog>> = lock(&self.catalogs).insert(tenant, Arc::new(initial));
        Ok(ts)
    }

    /// The initial catalog of `tenant`, held in memory after a drop of everything in it: `rdf:type` declared an edge
    /// when it has a UID, else empty, so the tenant's first write gives it the initial catalog.
    fn initial_catalog(&self, tenant: u64) -> Result<Catalog, ChangesetError> {
        Ok(match self.quads.id_of(tenant, rdf::TYPE.as_str())? {
            Some(rdf_type) => Catalog::initial(tenant, rdf_type),
            None => Catalog::new(tenant),
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

/// Every quad keyspace's prefix of graph `graph` of `tenant`.
fn graph_prefixes(tenant: u64, graph: Graph) -> Vec<Vec<u8>> {
    QUAD_KEYSPACES
        .iter()
        .flat_map(|k| parts(*k, |split| key_prefix(*k, split, tenant, Some(graph), None)))
        .collect()
}

/// Every quad and registration keyspace's prefix of `tenant`: no partition holds a catalog key.
fn tenant_prefixes(tenant: u64) -> Vec<Vec<u8>> {
    QUAD_KEYSPACES
        .iter()
        .chain(&REGISTRATION_KEYSPACES)
        .flat_map(|k| parts(*k, |split| key_prefix(*k, split, tenant, None, None)))
        .collect()
}

/// The first key past every key `prefix` begins: the prefix with its last byte below 0xff raised by one and what follows
/// it removed; empty, past the last key, when every byte is 0xff.
fn successor(prefix: &[u8]) -> Vec<u8> {
    let mut end = prefix.to_vec();
    // Ends: each turn removes one of the prefix's bytes.
    while let Some(last) = end.pop() {
        if last < u8::MAX {
            end.push(last + 1);
            return end;
        }
    }
    end
}
