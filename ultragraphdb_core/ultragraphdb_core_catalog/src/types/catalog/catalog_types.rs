/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use ultragraphdb_core_ids::SystemId;

use ultragraphdb_core_mapping::TypeDeclaration;

use crate::errors::catalog_error::CatalogError;
use crate::types::catalog::Catalog;

impl Catalog {
    /// Declares entity type `uid` with the parent `parent` names, `parent` being that parent's ID. Refused, changing
    /// nothing, when the type already has another parent, or the parent descends from the type.
    pub fn declare_type(
        &mut self,
        uid: SystemId,
        declaration: TypeDeclaration,
        parent: Option<SystemId>,
    ) -> Result<(), CatalogError> {
        let name = declaration.iri().as_str();
        if let (Some(held), Some(new)) = (
            self.types.get(&uid).and_then(TypeDeclaration::parent),
            declaration.parent(),
        ) && held != new
        {
            return Err(CatalogError::second_parent(
                name,
                held.as_str(),
                new.as_str(),
            ));
        }
        if let Some(parent_uid) = parent {
            let mut at = Some(parent_uid);
            while let Some(ancestor) = at {
                if ancestor == uid {
                    let parent_name = declaration.parent().map_or("", |p| p.as_str());
                    return Err(CatalogError::parent_cycle(name, parent_name));
                }
                at = self.parent_uid(ancestor);
            }
        }
        let _replaced: Option<TypeDeclaration> = self.types.insert(uid, declaration);
        Ok(())
    }

    /// The ID of type `uid`'s parent, when the parent is declared.
    fn parent_uid(&self, uid: SystemId) -> Option<SystemId> {
        let parent = self.types.get(&uid)?.parent()?;
        self.types
            .iter()
            .find(|(_, d)| d.iri() == parent)
            .map(|(parent_uid, _)| *parent_uid)
    }

    /// Each declared type's number in a preorder walk of the type hierarchy, from 1: each type before its children,
    /// roots and siblings in ID order, which is the order their IRIs were first named. A type's descendants then hold
    /// the numbers right after its own, as the ID type bits will.
    pub fn preorder(&self) -> BTreeMap<SystemId, u32> {
        let mut children: BTreeMap<Option<SystemId>, Vec<SystemId>> = BTreeMap::new();
        for uid in self.types.keys() {
            children
                .entry(self.parent_uid(*uid))
                .or_default()
                .push(*uid);
        }
        let mut numbers = BTreeMap::new();
        let mut stack: Vec<SystemId> = children
            .get(&None)
            .map(|roots| roots.iter().rev().copied().collect())
            .unwrap_or_default();
        while let Some(uid) = stack.pop() {
            let _numbered: Option<u32> = numbers.insert(uid, numbers.len() as u32 + 1);
            if let Some(kids) = children.get(&Some(uid)) {
                stack.extend(kids.iter().rev().copied());
            }
        }
        numbers
    }
}
