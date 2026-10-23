/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/mutation.go (runMutation, createSchema) and worker/draft.go (the edges of
 * applyMutations). Changed for ultragraphdb: translated to Rust; a predicate the edges create is defined in a schema
 * overlay of the request and written in its commit batch, where Dgraph writes it at the proposal's start timestamp
 * and keeps it after an abort (ledger 40); the edges' batches hold whole predicates and are applied in forks of the
 * request on the executor, where Dgraph's goroutines share one transaction.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use ultragraphdb_core_keys::data_key;
use ultragraphdb_core_schema::SchemaMode;
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb::metadata::HintType;
use ultragraphdb_proto_pb::pb::{DirectedEdge, Mutations, SchemaUpdate, directed_edge::Op};
use ultragraphdb_store_posting::{CachedList, Request};
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::utils::divide::edge_batches;
use crate::utils::index::{Maintain, add_mutation_with_index};
use crate::utils::validate::{is_delete_predicate_edge, type_of_edge, validate_and_convert};

/// A request applying edges: a request of the local commit, and the predicates its edges create.
#[derive(Debug)]
pub struct ApplyRequest<'a, S, C, E> {
    applier: &'a Applier<S, C, E>,
    request: Request<'a, S, C>,
    created: BTreeMap<String, SchemaUpdate>,
}

impl<'a, S, C, E> ApplyRequest<'a, S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    pub(crate) fn new(applier: &'a Applier<S, C, E>, request: Request<'a, S, C>) -> Self {
        Self {
            applier,
            request,
            created: BTreeMap::new(),
        }
    }

    pub fn read_ts(&self) -> u64 {
        self.request.read_ts()
    }

    /// The request of the local commit, to read its lists.
    pub fn request(&mut self) -> &mut Request<'a, S, C> {
        &mut self.request
    }

    /// The predicates the request's edges create, with the schema each gets.
    pub fn created(&self) -> &BTreeMap<String, SchemaUpdate> {
        &self.created
    }

    pub(crate) fn into_parts(self) -> (Request<'a, S, C>, BTreeMap<String, SchemaUpdate>) {
        (self.request, self.created)
    }

    /// Dgraph's `runMutation`: converts `edge` to its predicate's type and applies it with its index tokens, reverse
    /// edge and count keys.
    ///
    /// # Errors
    ///
    /// `runMutation: Unable to find schema for <attr>` for a set of a predicate without a schema; a predicate drop,
    /// which is no edge; the conversion's refusal; and index maintenance's errors.
    pub fn run_mutation(&mut self, edge: &DirectedEdge) -> Result<(), ApplyError> {
        run_mutation(self.applier, &self.created, &mut self.request, edge)
    }

    /// The edges of Dgraph's `applyMutations`: defines each predicate the edges set and the schema does not hold, in
    /// the request's overlay; sorts the edges by predicate and subject, stably; and applies them, a batch per
    /// predicate on the executor when there are enough of them.
    ///
    /// # Errors
    ///
    /// A predicate's definition refused, and the first error of each batch.
    pub fn apply_edges(&mut self, m: &Mutations) -> Result<(), ApplyError> {
        let mut first_types: BTreeMap<String, TypeId> = BTreeMap::new();
        for edge in &m.edges {
            if edge.op == Op::Del as i32 {
                continue;
            }
            first_types
                .entry(edge.attr.clone())
                .or_insert_with(|| type_of_edge(edge));
        }
        let hints = m.metadata.as_ref().map(|md| &md.pred_hints);
        for (attr, storage_type) in first_types {
            if self.applier.schema().type_of(&attr).is_ok() || self.created.contains_key(&attr) {
                continue;
            }
            let hint = hints
                .and_then(|h| h.get(&attr))
                .and_then(|h| HintType::try_from(*h).ok())
                .unwrap_or(HintType::Default);
            let mut su = self.create_schema(&attr, storage_type, hint);
            self.applier.check_schema(&mut su)?;
            self.created.insert(attr, su);
        }

        let mut edges = m.edges.clone();
        edges.sort_by(|a, b| a.attr.cmp(&b.attr).then(a.entity.cmp(&b.entity)));
        let batches = edge_batches(&edges);
        if batches.len() <= 1 {
            return edges
                .iter()
                .try_for_each(|e| run_mutation(self.applier, &self.created, &mut self.request, e));
        }
        let (applier, created) = (self.applier, &self.created);
        let tasks: Vec<_> = batches
            .into_iter()
            .map(|range| {
                let mut fork = self.request.fork();
                let batch = &edges[range];
                move || {
                    let result = batch
                        .iter()
                        .try_for_each(|e| run_mutation(applier, created, &mut fork, e));
                    (fork, result)
                }
            })
            .collect();
        let mut causes = Vec::new();
        for (fork, result) in applier.executor().run_all(tasks) {
            self.request.absorb(fork);
            if let Err(e) = result {
                causes.push(e);
            }
        }
        if causes.is_empty() {
            Ok(())
        } else {
            Err(ApplyError::Mutations(causes))
        }
    }

    /// Dgraph's `createSchema`: the schema a predicate the edges create gets: the type of its first set, a uid
    /// predicate a list, a `SINGLE` or `LIST` hint overriding the list flag. A schema being applied to it keeps its
    /// definition with the type replaced.
    fn create_schema(&self, attr: &str, typ: TypeId, hint: HintType) -> SchemaUpdate {
        if let Some(mut s) = self.applier.schema().get(SchemaMode::Write, attr) {
            s.value_type = typ.enum_value();
            return s;
        }
        let mut s = SchemaUpdate {
            predicate: attr.to_string(),
            value_type: typ.enum_value(),
            ..SchemaUpdate::default()
        };
        // Predicates of type uid were once all lists.
        if typ == TypeId::UID {
            s.list = true;
        }
        match hint {
            HintType::Single => s.list = false,
            HintType::List => s.list = true,
            HintType::Default => {}
        }
        s
    }
}

/// Dgraph's `runMutation` over `request`, the predicates the request creates in `created`.
pub(crate) fn run_mutation<S, C, E>(
    applier: &Applier<S, C, E>,
    created: &BTreeMap<String, SchemaUpdate>,
    request: &mut Request<'_, S, C>,
    edge: &DirectedEdge,
) -> Result<(), ApplyError>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    let schema = applier.index_schema(&edge.attr, created)?;
    let su = schema.schema().cloned();
    if edge.op != Op::Del as i32 && su.is_none() {
        return Err(ApplyError::NoSchema(&edge.attr));
    }
    if is_delete_predicate_edge(edge) {
        return Err(ApplyError::NeverReach());
    }
    let mut edge = edge.clone();
    validate_and_convert(&mut edge, &su.unwrap_or_default())?;

    // Dgraph's read strategy: a scalar, a language value, a count index and a delete read the list; anything else
    // writes it blind. A list written blind is completed before any later read of it.
    let key = data_key(&edge.attr, edge.entity)?;
    let reads = (edge.lang.is_empty() && !schema.is_list())
        || !edge.lang.is_empty()
        || schema.has_count()
        || edge.op == Op::Del as i32;
    if reads {
        request.get(&key)?;
    } else {
        request.get_from_delta(&key);
    }
    let mut draw = || applier.draw();
    let mut m = Maintain {
        schema: &schema,
        indexing: applier.schema().indexing_in_progress(),
        draw: &mut draw,
    };
    add_mutation_with_index(request, &mut m, &edge)
}
