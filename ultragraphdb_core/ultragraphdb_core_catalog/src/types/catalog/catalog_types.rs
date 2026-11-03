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
            // The walk up the hierarchy visits each declared type at most once, so it ends within as many steps as
            // there are types, plus the parent itself; a chain longer than that holds a cycle, which a catalog
            // assembled from stored entries could, and is refused as one.
            let parent_name = declaration.parent().map_or("", |p| p.as_str());
            let mut at = Some(parent_uid);
            for _ in 0..=self.types.len() {
                let Some(ancestor) = at else {
                    break;
                };
                if ancestor == uid {
                    return Err(CatalogError::parent_cycle(name, parent_name));
                }
                at = self.parent_uid(ancestor);
            }
            if at.is_some() {
                return Err(CatalogError::parent_cycle(name, parent_name));
            }
        }
        let _replaced: Option<TypeDeclaration> = self.types.insert(uid, declaration);
        debug_assert!(self.types.contains_key(&uid));
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
        // Each type is pushed once, from its one parent's children or as a root, so the walk takes at most as many
        // turns as there are types; a type on a cycle is reached from no root and is left unnumbered.
        while let Some(uid) = stack.pop() {
            debug_assert!(!numbers.contains_key(&uid));
            let next = u32::try_from(numbers.len()).map_or(u32::MAX, |n| n.saturating_add(1));
            let _numbered: Option<u32> = numbers.insert(uid, next);
            if let Some(kids) = children.get(&Some(uid)) {
                stack.extend(kids.iter().rev().copied());
            }
        }
        numbers
    }
}
