/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (DeleteAll, DeleteAllForNs, DeleteData, DeletePredicate,
 * DeleteNamespace), worker/draft.go (the drops of applyMutations and its DeleteNs proposal) and schema/schema.go
 * (DeleteType, PredicatesToDelete). Changed for ultragraphdb: translated to Rust; every drop removes what it drops
 * physically through the posting lists' drop, at a timestamp taken under their writer, split parts included (ledger
 * 7), a namespace's everything its schema and type keys too (ledger 8), and every rollup read before it is discarded
 * (ledger 38); a request that read below a drop it writes under is refused at its commit, where Dgraph aborts its
 * pending transactions; a rebuild of a predicate dropped is cancelled.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_keys::{ROOT_NAMESPACE, parse_namespace, type_key};
use ultragraphdb_core_catalog::{initial_schema, initial_types, schema_entry, type_entry};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{Entry, Executor, ListCache, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::utils::lock::lock;
use crate::utils::prefixes::{
    namespace_data_prefixes, namespace_prefixes, predicate_drop_prefixes,
};

impl<S, C, E> Applier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    /// Dgraph's `DeletePredicate`: drops the predicate with its schema, and for a vector predicate the three the
    /// index keeps beside it, each with its split parts: the drop's timestamp.
    pub fn drop_predicate(&self, attr: &str) -> Result<u64, ApplyError> {
        let _apply = lock(&self.apply);
        // A predicate without a schema still drops its keys, which deletes written without one leave; Dgraph's
        // `PredicatesToDelete` names none and drops nothing.
        let mut preds = self.schema.predicates_to_delete(attr);
        if preds.is_empty() {
            preds.push(attr.to_string());
        }
        tracing::info!(attr, "Dropping predicate");
        self.cancel_rebuilds(|a| preds.iter().any(|p| p == a));
        let mut prefixes = Vec::new();
        for p in &preds {
            self.schema.remove_predicate(p);
            prefixes.extend(predicate_drop_prefixes(p)?);
        }
        let ts = self.lists.drop_prefixes_now(&prefixes, Vec::new())?;
        self.fence(prefixes, ts);
        Ok(ts)
    }

    /// Dgraph's `DeleteType`: drops the type's key: the drop's timestamp.
    pub fn drop_type(&self, type_name: &str) -> Result<u64, ApplyError> {
        let _apply = lock(&self.apply);
        self.schema.remove_type(type_name);
        Ok(self
            .lists
            .drop_prefixes_now(&[type_key(type_name)?], Vec::new())?)
    }

    /// Dgraph's `DeleteData`: drops the namespace's data, its split parts with it, and keeps its schema and types: the
    /// drop's timestamp. A rebuild in the namespace is cancelled, its predicate keeping the previous schema.
    pub fn drop_data(&self, ns: u64) -> Result<u64, ApplyError> {
        let _apply = lock(&self.apply);
        for (attr, old) in self.cancel_rebuilds(|a| in_namespace(a, ns)) {
            self.schema.set(&attr, old);
            self.schema.delete_mut_schema(&attr);
        }
        let prefixes = namespace_data_prefixes(ns);
        let ts = self.lists.drop_prefixes_now(&prefixes, Vec::new())?;
        self.fence(prefixes, ts);
        Ok(ts)
    }

    /// Dgraph's `DeleteAllForNs` and what its proposal writes after it: drops everything the namespace holds, its
    /// schema and type keys included, then writes its initial schema and types at timestamp 1: the drop's timestamp.
    pub fn drop_all_in_ns(&self, ns: u64) -> Result<u64, ApplyError> {
        let _apply = lock(&self.apply);
        self.schema.delete_preds_for_ns(ns);
        self.cancel_rebuilds(|a| in_namespace(a, ns));
        let (entries, initial) = initial_entries(ns)?;
        let prefixes = namespace_prefixes(ns);
        let ts = self.lists.drop_prefixes_now(&prefixes, entries)?;
        self.set_initial(initial);
        self.fence(prefixes, ts);
        Ok(ts)
    }

    /// Dgraph's `DeleteAll` and what its proposal writes after it: drops every key, then writes the root namespace's
    /// initial schema and types at timestamp 1: the drop's timestamp.
    pub fn drop_all(&self) -> Result<u64, ApplyError> {
        let _apply = lock(&self.apply);
        self.schema.delete_all();
        self.cancel_rebuilds(|_| true);
        let (entries, initial) = initial_entries(ROOT_NAMESPACE)?;
        let ts = self.lists.drop_all_now(entries)?;
        self.set_initial(initial);
        self.fence(vec![Vec::new()], ts);
        Ok(ts)
    }

    /// Dgraph's `DeleteNamespace`: forgets the namespace's predicates and types and bans it, so the store drops every
    /// later write to it and serves none of its keys: the newest timestamp.
    ///
    /// # Errors
    ///
    /// The root namespace, which Dgraph asserts is never deleted, and the store's error.
    pub fn delete_namespace(&self, ns: u64) -> Result<u64, ApplyError> {
        if ns == ROOT_NAMESPACE {
            return Err(ApplyError::RootNamespace());
        }
        let _apply = lock(&self.apply);
        self.schema.delete_preds_for_ns(ns);
        self.cancel_rebuilds(|a| in_namespace(a, ns));
        self.lists.store().ban_namespace(ns)?;
        self.lists.advance_epoch();
        Ok(self.lists.latest_ts())
    }

    /// Sets the initial schema and types a drop wrote.
    fn set_initial(&self, initial: Initial) {
        for s in initial.0 {
            self.schema.set(&s.predicate.clone(), s);
        }
        for t in initial.1 {
            self.schema.set_type(&t.type_name.clone(), t);
        }
    }
}

/// A namespace's initial schema and types.
type Initial = (
    Vec<ultragraphdb_proto_pb::pb::SchemaUpdate>,
    Vec<ultragraphdb_proto_pb::pb::TypeUpdate>,
);

/// The entries of a namespace's initial schema and types at timestamp 1, and the definitions.
fn initial_entries(ns: u64) -> Result<(Vec<Entry>, Initial), ApplyError> {
    let schema = initial_schema(ns, false);
    let types = initial_types(ns, false);
    let mut entries = Vec::with_capacity(schema.len() + types.len());
    for s in &schema {
        entries.push(schema_entry(s, 1)?);
    }
    for t in &types {
        entries.push(type_entry(t, 1)?);
    }
    Ok((entries, (schema, types)))
}

fn in_namespace(attr: &str, ns: u64) -> bool {
    parse_namespace(attr).is_ok_and(|n| n == ns)
}
