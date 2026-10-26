/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, worker/mutation.go (runSchemaMutation, checkSchema, validateSchemaForUnique, hasEdges,
 * updateSchema) and edgraph/server.go (the refusal of a change while an index is built). Changed for ultragraphdb:
 * translated to Rust; every update of a change is checked before any is applied, the list-to-scalar refusal
 * included, so a refused change alters nothing, where Dgraph refuses an empty list's change after dropping its
 * indexes; a predicate holds data when one of its data lists is non-empty, where Dgraph's `hasEdges` counts any key
 * whose newest version is not empty; an HNSW option below 1 is refused, where Dgraph accepts it and panics at the
 * first search; a new predicate's schema is written at the change's commit timestamp, where Dgraph writes it at the
 * start timestamp.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use ultragraphdb_core_keys::is_internal_predicate;
use ultragraphdb_core_catalog::{SchemaMode, has_tokenizer_or_vector_index_spec, schema_entry};
use ultragraphdb_core_tok::get_factory_create_spec_from_spec;
use ultragraphdb_core_types::TypeId;
use ultragraphdb_proto_pb::pb::schema_update::Directive;
use ultragraphdb_proto_pb::pb::{SchemaUpdate, posting};
use ultragraphdb_store_hnsw::PersistentHnsw;
use ultragraphdb_store_posting::{CachedList, List};
use ultragraphdb_traits_store::{Executor, ListCache, Scan, Store, StoreSnapshot};

use crate::errors::apply_error::ApplyError;
use crate::types::applier::Applier;
use crate::types::applier::applier_mutations::first_result;
use crate::types::index_op::IndexOp;
use crate::types::index_rebuild::IndexRebuild;
use crate::utils::edge_text::{schema_text, val_type_name};
use crate::utils::lock::lock;
use crate::utils::prefixes::{attr_data_prefix, predicate_drop_prefixes, vector_edge_prefixes};
use crate::utils::validate::attr_name;

impl<S, C, E> Applier<S, C, E>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    E: Executor + Send + Sync + 'static,
{
    /// Dgraph's `runSchemaMutation`: applies each update. A new predicate's schema is written at once; a predicate's
    /// change sets the interim schema queries use and the schema writes use, and builds the indexes it adds on the
    /// executor, writes maintaining the previous schema's indexes as well as the new schema's until one batch swaps
    /// them; a change that builds no index, and a vector index's build, are applied before this returns. Returns the
    /// timestamp of the last update's change.
    ///
    /// # Errors
    ///
    /// `errIndexingInProgress. Please retry` while an index is built; an update `checkSchema` refuses, a list
    /// changed to a scalar, and the errors of a change applied at once; nothing is changed when any update is refused.
    pub fn alter(self: &Arc<Self>, updates: Vec<SchemaUpdate>) -> Result<u64, ApplyError> {
        let _apply = lock(&self.apply);
        if self.schema.indexing_in_progress() || self.rebuilding() {
            return Err(ApplyError::IndexingInProgress());
        }
        let mut checked = Vec::with_capacity(updates.len());
        for mut su in updates {
            self.check_schema(&mut su)?;
            let old = self.schema.get(SchemaMode::Query, &su.predicate);
            if old.is_some() {
                IndexRebuild::new(&su.predicate, 0, old.clone(), su.clone())
                    .needs_list_type_rebuild()?;
            }
            checked.push((su, old));
        }
        let mut ts = 0;
        for (su, old) in checked {
            ts = match old {
                None => self.define(su)?,
                Some(old) => self.change(su, old)?,
            };
        }
        Ok(ts)
    }

    /// Writes the schema of a predicate the state does not hold: Dgraph's `updateSchema`.
    fn define(&self, su: SchemaUpdate) -> Result<u64, ApplyError> {
        let attr = su.predicate.clone();
        let change_ts = self.lists.fresh_ts();
        self.fence(change_prefixes(&attr)?, change_ts);
        let mut request = self.lists.request(self.lists.latest_ts());
        request.add_entry(schema_entry(&su, 0)?);
        let mut batch = self.lists.batch();
        batch.push(request);
        let results = self.lists.commit_with(batch, |_| {
            self.schema.set(&attr, su.clone());
            self.schema.delete_mut_schema(&attr);
            Ok::<_, ApplyError>(())
        })?;
        first_result(results)
    }

    /// Changes the schema of a predicate the state holds as `old`.
    fn change(self: &Arc<Self>, su: SchemaUpdate, old: SchemaUpdate) -> Result<u64, ApplyError> {
        let attr = su.predicate.clone();
        let rebuild = IndexRebuild::new(&attr, 0, Some(old.clone()), su.clone());
        let query = rebuild.get_query_schema()?;
        // A write during a build reads only lists the build does not replace, or it maintains them from what they
        // held before: a vector index is built in place; a count kept beside reverse edges being built reads reverse
        // lists the build has not written yet; a list-type rewrite changes the identities of the values a write
        // reads. Those changes are built while the change holds every commit back.
        let reads_rebuilt = (rebuild.needs_reverse_edges_rebuild() == IndexOp::Rebuild
            && (su.count || old.count))
            || rebuild.needs_list_type_rebuild()?;
        let in_background = rebuild.need_index_rebuild()?
            && !reads_rebuilt
            && rebuild
                .needs_tok_index_rebuild()?
                .vector_indexes_to_rebuild()
                .is_empty();
        let fenced = change_prefixes(&attr)?;
        // The schemas writes and queries use, before the change's timestamp is taken: a request reading at or above
        // it applies its edges under them.
        self.schema.set(&attr, query);
        self.schema.set_mut_schema(&attr, su.clone());
        let id = self.register_rebuild(&attr, old, su);
        self.lists.advance_epoch();
        let change_ts = self.lists.fresh_ts();
        self.fence(fenced, change_ts);
        self.lists.pause_rollups();
        let rebuild = rebuild.at(change_ts);
        if let Err(error) = self.record_rebuild(&rebuild) {
            self.abandon(id, &rebuild);
            return Err(error);
        }
        if in_background {
            let me = Arc::clone(self);
            self.executor.spawn(move || me.run_job(id, rebuild));
            return Ok(change_ts);
        }
        self.build_and_swap(id, &rebuild)?;
        Ok(change_ts)
    }

    /// Dgraph's `checkSchema`, with the port's refusal of an HNSW option below 1. A `@unique` predicate gains
    /// `@upsert`.
    ///
    /// # Errors
    ///
    /// Dgraph's refusals: no predicate, an internal one, an index without a tokenizer and the reverse, an index on a
    /// uid and a reverse of a scalar, `@upsert` without an index, the `@unique` rules, a change to or from
    /// `password`, a list holding data changed to a scalar, and a change between uid and a scalar while the predicate
    /// holds data; and a vector index option its factory refuses.
    pub(crate) fn check_schema(&self, su: &mut SchemaUpdate) -> Result<(), ApplyError> {
        let attr = attr_name(&su.predicate);
        if attr.is_empty() {
            return Err(ApplyError::NoPredicate());
        }
        if is_internal_predicate(&su.predicate)? {
            return Err(ApplyError::InternalName(attr));
        }
        let index = su.directive == Directive::Index as i32;
        if index && !has_tokenizer_or_vector_index_spec(su) {
            return Err(ApplyError::TokenizerRequired(schema_text(su)));
        }
        if has_tokenizer_or_vector_index_spec(su) && !index {
            return Err(ApplyError::DirectiveRequired());
        }
        let typ = TypeId::new(su.value_type);
        if typ == TypeId::UID && index {
            return Err(ApplyError::IndexOnUid(attr));
        } else if typ != TypeId::UID && su.directive == Directive::Reverse as i32 {
            return Err(ApplyError::ReverseNonUid(attr));
        }
        if su.upsert && su.tokenizer.is_empty() && !su.unique {
            return Err(ApplyError::UpsertNeedsIndex(attr));
        }
        if su.unique {
            let prev = self.schema.get(SchemaMode::Query, &su.predicate);
            validate_schema_for_unique(prev.as_ref(), su)?;
        }
        for spec in &su.index_specs {
            let fcs = get_factory_create_spec_from_spec(spec)?;
            let bits = fcs.factory().float_bits();
            let built = if bits == 64 {
                PersistentHnsw::<f64>::new(&su.predicate, fcs.opts(), bits).map(|_| ())
            } else {
                PersistentHnsw::<f32>::new(&su.predicate, fcs.opts(), bits).map(|_| ())
            };
            if let Err(e) = built {
                return Err(ApplyError::VectorOption(
                    fcs.factory().name(),
                    e.to_string(),
                ));
            }
        }

        let Ok(t) = self.schema.type_of(&su.predicate) else {
            // No schema before: no conversion to check.
            return Ok(());
        };
        if t.is_scalar() && (t == TypeId::PASSWORD || typ == TypeId::PASSWORD) {
            if t != typ {
                return Err(ApplyError::PasswordChange(
                    val_type_name(t.enum_value()),
                    val_type_name(typ.enum_value()),
                ));
            }
        } else if t.is_scalar() == typ.is_scalar() {
            if self.schema.is_list(&su.predicate) && !su.list && self.has_data(&su.predicate)? {
                return Err(ApplyError::ListToScalar(t.name(), typ.name(), attr));
            }
        } else if self.has_data(&su.predicate)? {
            return Err(ApplyError::ScalarUid(attr));
        }
        Ok(())
    }

    /// Whether `attr` holds data: one of its data lists is non-empty at the newest timestamp. Dgraph's `hasEdges`
    /// counts any key whose newest version is not empty, by its own note an approximation (`mutation.go:355-376`).
    pub(crate) fn has_data(&self, attr: &str) -> Result<bool, ApplyError> {
        let read_ts = self.lists.latest_ts();
        self.lists.gate(read_ts)?;
        let snapshot = self.lists.store().snapshot(read_ts)?;
        for item in snapshot.scan(&Scan::prefix(attr_data_prefix(attr)?)) {
            let item = item?;
            let (list, _) = List::read(item.key(), &snapshot)?;
            if !list.is_empty(read_ts, 0)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// The prefixes a change of `attr`'s schema fences: every key of the predicate and of the predicates its vector index
/// keeps, and its schema key.
fn change_prefixes(attr: &str) -> Result<Vec<Vec<u8>>, ApplyError> {
    let mut prefixes = predicate_drop_prefixes(attr)?;
    prefixes.extend(vector_edge_prefixes(attr)?);
    Ok(prefixes)
}

/// Dgraph's `validateSchemaForUnique`.
fn validate_schema_for_unique(
    prev: Option<&SchemaUpdate>,
    current: &mut SchemaUpdate,
) -> Result<(), ApplyError> {
    let valid_tokenizer = |tokenizers: &[String]| {
        tokenizers
            .iter()
            .any(|v| v == "hash" || v == "exact" || v == "int")
    };
    let attr = attr_name(&current.predicate);
    if let Some(prev) = prev.filter(|p| !p.predicate.is_empty()) {
        if prev.upsert && !current.upsert {
            return Err(ApplyError::DropUpsert(attr));
        }
        if prev.unique && !valid_tokenizer(&current.tokenizer) {
            return Err(ApplyError::DropUniqueIndex(prev.tokenizer.clone(), attr));
        }
    }
    current.upsert = true;
    match posting::ValType::try_from(current.value_type) {
        Ok(posting::ValType::String) => {
            if current.tokenizer.is_empty() || !valid_tokenizer(&current.tokenizer) {
                Err(ApplyError::UniqueStringIndex(attr))
            } else {
                Ok(())
            }
        }
        Ok(posting::ValType::Int) => {
            if current.tokenizer.is_empty() {
                Err(ApplyError::UniqueIntIndex(attr))
            } else {
                Ok(())
            }
        }
        _ => Err(ApplyError::UniqueUnsupported(val_type_name(
            current.value_type,
        ))),
    }
}
