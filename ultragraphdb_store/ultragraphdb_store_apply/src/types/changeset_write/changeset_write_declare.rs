/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{Catalog, ENTITY_TYPE, PREDICATE};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::DEFAULT_GRAPH;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_store_hnsw::{PersistentHnsw, VectorKeys};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::index_plan::IndexPlan;
use crate::types::rebuild::Rebuild;
use crate::utils::catalog_keys::{catalog_key, declaration_key};
use crate::utils::quad_prefixes::{count_prefixes, tokenizer_prefixes, vector_prefixes};
use crate::utils::vector_index::hnsw;

impl<S, C, B> ChangesetWrite<'_, S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// Applies a declaration to the write's catalog, with its catalog entry; a type's parent is named by its UID.
    pub(crate) fn declare(&mut self, declaration: &Declaration) -> Result<(), ChangesetError> {
        let (uid, parent) = match declaration {
            Declaration::Predicate(p) => {
                let uid = self.iri(p.predicate().as_ref())?;
                return self.declare_predicate(uid, p);
            }
            Declaration::EntityType(t) => {
                let parent = t.parent().map(|p| self.iri(p.as_ref())).transpose()?;
                (self.iri(t.iri().as_ref())?, parent)
            }
            Declaration::Relation(r) => (self.iri(r.iri().as_ref())?, None),
        };
        self.catalog.declare(uid, declaration.clone(), parent)?;
        self.add_entry(&declaration_key(self.namespace(), uid, declaration));
        Ok(())
    }

    /// Declares predicate `uid` as `new`, checked against its data. A declaration adding an index to a predicate holding
    /// data starts a build: the catalog holds the interim declaration until the build swaps the new one in, and the
    /// rest of the changeset writes the predicate under the declaration the build maintains. A declaration removing an
    /// index without building one removes its keys after the changeset.
    fn declare_predicate(
        &mut self,
        uid: SystemId,
        new: &PredicateDeclaration,
    ) -> Result<(), ChangesetError> {
        let iri = new.predicate().as_str();
        if self.maintained.as_ref().is_some_and(|(m, _)| *m == uid) {
            return Err(ChangesetError::rebuilding(iri));
        }
        Catalog::check_predicate(new)?;
        if let Some(spec) = new.vector_index() {
            if spec.dimension().is_none() {
                return Err(ChangesetError::no_dimension(iri));
            }
            let _index: PersistentHnsw<f32> = hnsw(
                spec,
                VectorKeys::new(iri, self.namespace(), DEFAULT_GRAPH, uid),
            )?;
        }
        let old = self
            .catalog
            .predicate_or_default(uid, new.predicate())
            .into_owned();
        self.check_data(uid, &old, new)?;
        let plan = IndexPlan::of(&old, new);
        let declared = if plan.builds() && self.holds_data(uid)? {
            if self.building {
                return Err(ChangesetError::rebuilding(iri));
            }
            let rebuild = Rebuild::pending(self.namespace(), uid, old, new.clone());
            self.maintained = Some((uid, rebuild.maintained()));
            self.vector_built = rebuild.plan.build_vector.then_some(uid);
            self.building = true;
            let interim = rebuild.interim();
            self.pending = Some(rebuild);
            interim
        } else {
            self.remove_indexes(uid, &plan);
            new.clone()
        };
        self.catalog.declare_predicate(uid, declared)?;
        self.add_entry(&catalog_key(self.namespace(), PREDICATE, uid));
        Ok(())
    }

    /// Removes predicate `uid`'s declaration, named `iri`: declared the default first, which checks nothing against the
    /// data and removes every index the declaration names, then left out of the catalog.
    pub(crate) fn undeclare(
        &mut self,
        uid: SystemId,
        iri: &NamedNode,
    ) -> Result<(), ChangesetError> {
        self.declare_predicate(uid, &PredicateDeclaration::new(iri.clone()))?;
        let _declared: bool = self.catalog.remove_predicate(uid);
        debug_assert!(self.catalog.predicate(uid).is_none());
        Ok(())
    }

    /// Removes entity type or relation `uid`'s declaration, of entry kind `kind`, as a version no longer naming it asks.
    pub(crate) fn undeclare_named(&mut self, kind: u8, uid: SystemId) {
        let removed = if kind == ENTITY_TYPE {
            self.catalog.remove_entity_type(uid)
        } else {
            self.catalog.remove_relation(uid)
        };
        if removed {
            self.add_entry(&catalog_key(self.namespace(), kind, uid));
        }
        debug_assert!(self.catalog.entity_type(uid).is_none() || kind != ENTITY_TYPE);
    }

    /// Records the keys of the indexes `plan` removes from predicate `uid`, which a request after the write deletes.
    fn remove_indexes(&mut self, uid: SystemId, plan: &IndexPlan) {
        let ns = self.namespace();
        for t in &plan.remove_tokenizers {
            self.removals.extend(tokenizer_prefixes(ns, uid, t));
        }
        if plan.remove_count {
            self.removals.extend(count_prefixes(ns, uid));
        }
        if plan.remove_vector {
            self.removals.extend(vector_prefixes(ns, uid));
        }
    }
}
