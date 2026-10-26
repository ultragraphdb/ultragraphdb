/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::Declaration;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Applies a declaration to the write's catalog, with its catalog entry; a type's parent is named by its UID.
    pub(crate) fn declare(&mut self, declaration: &Declaration) -> Result<(), ChangesetError> {
        let (uid, parent) = match declaration {
            Declaration::Predicate(p) => (self.iri(p.predicate().as_ref())?, None),
            Declaration::EntityType(t) => {
                let parent = t.parent().map(|p| self.iri(p.as_ref())).transpose()?;
                (self.iri(t.iri().as_ref())?, parent)
            }
            Declaration::Relation(r) => (self.iri(r.iri().as_ref())?, None),
        };
        self.catalog.declare(uid, declaration.clone(), parent)?;
        let entry = Catalog::entry(self.namespace(), uid, declaration, 0);
        self.add_entry(entry);
        Ok(())
    }
}
