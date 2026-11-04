/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog versions of the metadata group in a partition: a version loaded at start, before replay, as the store
//! already reflects it; and a version taking effect at its epoch, executed as the round that installs it, its
//! declarations checked against the data and its index builds started.

use std::collections::BTreeSet;
use std::sync::Arc;

use oxrdf::NamedNode;
use oxrdf::vocab::rdf;
use ultragraphdb_core_catalog::{
    Catalog, CatalogContents, ENTITY_TYPE, RELATION, encode_declaration,
};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_mapping::{Change, Changeset, Declaration, PredicateDeclaration};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_posting::Writes;
use ultragraphdb_store_quads::{QuadWrite, WriteTerms};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_catalog::DefaultEntry;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot, VersionlessEntry};

use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};
use crate::types::catalog_execution::CatalogExecution;
use crate::types::changeset_core::ChangesetCore;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::execution::Execution;
use crate::utils::lock::lock;

/// Whether `error` refuses one declaration of a version, leaving its predicate as it was, rather than the version's
/// taking effect: the data contradicts it, the catalog refuses it, it is a second build of the version, or its vector
/// index is unusable. Any other error is a failure of the store or the code.
fn refuses_declaration(error: &ChangesetError) -> bool {
    match &error.0 {
        ChangesetErrorEnum::Contradicted { .. }
        | ChangesetErrorEnum::Catalog(_)
        | ChangesetErrorEnum::Rebuilding { .. }
        | ChangesetErrorEnum::VectorOption { .. }
        | ChangesetErrorEnum::Dimension { .. }
        | ChangesetErrorEnum::NoDimension { .. }
        | ChangesetErrorEnum::Hnsw(_) => true,
        ChangesetErrorEnum::Mapping(_)
        | ChangesetErrorEnum::Quad(_)
        | ChangesetErrorEnum::Posting(_)
        | ChangesetErrorEnum::Store(_)
        | ChangesetErrorEnum::Tok(_)
        | ChangesetErrorEnum::Invariant(_)
        | ChangesetErrorEnum::Literal { .. }
        | ChangesetErrorEnum::Unique { .. }
        | ChangesetErrorEnum::NoVectorIndex { .. }
        | ChangesetErrorEnum::OperationFailed { .. }
        | ChangesetErrorEnum::NoIndex { .. }
        | ChangesetErrorEnum::NotUnique { .. } => false,
    }
}

impl<S, C, B> ChangesetCore<S, C, B>
where
    S: Store,
    C: ListCache<Value = CachedList>,
    B: BlockSource,
{
    /// Installs `contents` as its namespace's catalog, as [`crate::ChangesetApplier::load_catalog`] does.
    pub fn load_catalog(&self, contents: &CatalogContents) -> Result<(), ChangesetError> {
        let namespace = contents.namespace();
        let id_of = |iri: &NamedNode| -> Result<SystemId, ChangesetError> {
            self.quads.id_of(namespace, iri.as_str())?.ok_or_else(|| {
                StoreError::corrupted(
                    iri.as_str().as_bytes().to_vec(),
                    format!(
                        "the catalog version declares <{iri}>, which the dictionary does not hold",
                        iri = iri.as_str()
                    ),
                )
                .into()
            })
        };
        let mut catalog = match self.quads.id_of(namespace, rdf::TYPE.as_str())? {
            Some(rdf_type) => Catalog::initial(namespace, rdf_type),
            None => Catalog::new(namespace),
        };
        for declaration in contents.declarations() {
            let (uid, parent) = match declaration {
                Declaration::Predicate(p) => (id_of(p.predicate())?, None),
                Declaration::EntityType(t) => (id_of(t.iri())?, t.parent().map(id_of).transpose()?),
                Declaration::Relation(r) => (id_of(r.iri())?, None),
            };
            catalog.declare(uid, declaration.clone(), parent)?;
        }
        let catalog = catalog.with_version(contents.version());
        debug_assert_eq!(catalog.namespace(), namespace);
        let _previous: Option<Arc<Catalog>> =
            lock(&self.catalogs).insert(namespace, Arc::new(catalog));
        Ok(())
    }

    /// The default-entry records of `namespace` to send at start, as [`crate::ChangesetApplier::reconcile_defaults`]
    /// gives them.
    pub fn reconcile_defaults(&self, namespace: u64) -> Result<Vec<DefaultEntry>, ChangesetError> {
        let read_ts = self.quads.lists().applied_ts();
        let snapshot = self.quads.store().snapshot(read_ts)?;
        let catalog = self.catalog(namespace)?;
        let mut held: BTreeSet<SystemId> = BTreeSet::new();
        let scan = Scan::prefix(key_prefix(
            Keyspace::Statistics,
            false,
            namespace,
            None,
            None,
        ));
        // The scan ends at the namespace's last statistics key; each key is read once.
        for item in snapshot.scan(&scan) {
            let key = item?.key().to_vec();
            let Ok((
                QuadKey::Statistics {
                    graph, predicate, ..
                },
                _,
            )) = QuadKey::parse(&key)
            else {
                continue;
            };
            let undeclared = catalog
                .predicate(predicate)
                .is_none_or(|d| *d == PredicateDeclaration::new(d.predicate().clone()));
            if undeclared
                && !held.contains(&predicate)
                && self
                    .quads
                    .statistics(namespace, graph, predicate, read_ts)?
                    .statements()
                    > 0
            {
                let _new: bool = held.insert(predicate);
            }
        }
        held.into_iter()
            .map(|predicate| {
                let iri = self.quads.iri_of(namespace, predicate)?.0;
                let default = PredicateDeclaration::new(NamedNode::new_unchecked(iri.clone()));
                let entry = encode_declaration(&Declaration::Predicate(default));
                Ok(DefaultEntry::new(namespace, iri, entry))
            })
            .collect()
    }

    /// The round installing `contents`, as [`crate::ChangesetApplier::execute_catalog`] executes it.
    pub fn execute_catalog(
        &self,
        contents: &CatalogContents,
        dictionary: &[&VersionlessEntry],
    ) -> Result<CatalogExecution, ChangesetError> {
        if let Some(running) = lock(&self.rebuild).as_ref() {
            return Err(ChangesetError::rebuilding(running.new.predicate().as_str()));
        }
        let namespace = contents.namespace();
        let current = (*self.catalog(namespace)?).clone();
        let read_ts = self.quads.lists().applied_ts();
        let mut write = QuadWrite::new(&self.quads, namespace, self.quads.lists().request(read_ts));
        let mut terms = WriteTerms::new(namespace);
        terms.inherit(dictionary);
        write.seed(&Writes::default(), terms)?;
        let mut execution = Execution::new(
            namespace,
            read_ts,
            ChangesetWrite::new(write, current.clone()),
        );
        let refusals = match self.declare_version(&mut execution, contents, &current) {
            Ok(refusals) => refusals,
            Err(failed) => {
                self.quads.release(execution.terms());
                return Err(failed);
            }
        };
        let mut executed = execution.finish()?;
        executed.catalog_version = Some(contents.version());
        executed.catalog_changed = true;
        Ok(CatalogExecution::new(executed, refusals))
    }

    /// Declares in `execution` every declaration of `contents`, and removes every predicate, entity type and relation
    /// `current` declares that the version does not, but for `rdf:type` and the implicit default entries: the
    /// declarations refused, a refusal of the data or the catalog leaving its predicate as it was.
    fn declare_version(
        &self,
        execution: &mut Execution<'_, S, C, B>,
        contents: &CatalogContents,
        current: &Catalog,
    ) -> Result<Vec<ChangesetError>, ChangesetError> {
        let namespace = contents.namespace();
        let mut refusals = Vec::new();
        for declaration in contents.declarations() {
            let changeset = Changeset::new(namespace).with(Change::Declare(declaration.clone()));
            match execution.apply(&changeset) {
                Ok(()) => {}
                Err(refused) if refuses_declaration(&refused) => refusals.push(refused),
                Err(failed) => return Err(failed),
            }
        }
        let declared: BTreeSet<&str> = contents
            .declarations()
            .iter()
            .filter_map(|d| match d {
                Declaration::Predicate(p) => Some(p.predicate().as_str()),
                Declaration::EntityType(_) | Declaration::Relation(_) => None,
            })
            .collect();
        for (uid, held) in current.predicates() {
            let iri = held.predicate();
            let implicit = *held == PredicateDeclaration::new(iri.clone());
            if !implicit && iri.as_ref() != rdf::TYPE && !declared.contains(iri.as_str()) {
                execution.undeclare(uid, iri)?;
            }
        }
        let named: BTreeSet<(u8, &str)> = contents
            .declarations()
            .iter()
            .filter_map(|d| match d {
                Declaration::EntityType(t) => Some((ENTITY_TYPE, t.iri().as_str())),
                Declaration::Relation(r) => Some((RELATION, r.iri().as_str())),
                Declaration::Predicate(_) => None,
            })
            .collect();
        let types = current
            .entity_types()
            .map(|(uid, t)| (ENTITY_TYPE, uid, t.iri().as_str()));
        let relations = current
            .relations()
            .map(|(uid, r)| (RELATION, uid, r.iri().as_str()));
        for (kind, uid, iri) in types.chain(relations) {
            if !named.contains(&(kind, iri)) {
                execution.undeclare_named(kind, uid);
            }
        }
        Ok(refusals)
    }
}
