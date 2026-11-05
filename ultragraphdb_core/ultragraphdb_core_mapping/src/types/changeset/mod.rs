/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::changes::Changes;
use crate::types::change::Change;
use crate::types::change_ref::ChangeRef;

/// A write as the engines produce it and storage applies it: a tenant and its changes, applied in order, whole or
/// not at all. Its terms are RDF terms; storage resolves them to term IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Changeset {
    tenant: u64,
    changes: Vec<Change>,
}

impl Changeset {
    /// An empty changeset for `tenant`.
    pub fn new(tenant: u64) -> Self {
        Self {
            tenant,
            changes: Vec::new(),
        }
    }

    /// Appends `change`.
    pub fn push(&mut self, change: Change) {
        self.changes.push(change);
    }

    /// The changeset with `change` appended.
    pub fn with(mut self, change: Change) -> Self {
        self.changes.push(change);
        self
    }

    /// The tenant the changes apply to.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The changes, in order.
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    /// The changes, in order, consuming the changeset.
    pub fn into_changes(self) -> Vec<Change> {
        self.changes
    }

    /// Whether it holds no change.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

impl Changes for Changeset {
    fn tenant(&self) -> u64 {
        Changeset::tenant(self)
    }

    fn changes(&self) -> impl Iterator<Item = ChangeRef<'_>> {
        Changeset::changes(self).iter().map(Change::as_ref)
    }
}
