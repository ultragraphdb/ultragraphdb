/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::traits::changes::Changes;
use crate::types::change::Change;
use crate::types::change_ref::ChangeRef;

/// A write as the engines produce it and storage applies it: a namespace and its changes, applied in order, whole or
/// not at all. Its terms are RDF terms; storage resolves them to term IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Changeset {
    namespace: u64,
    changes: Vec<Change>,
}

impl Changeset {
    /// An empty changeset for `namespace`.
    pub fn new(namespace: u64) -> Self {
        Self {
            namespace,
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

    /// The namespace the changes apply to.
    pub fn namespace(&self) -> u64 {
        self.namespace
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
    fn namespace(&self) -> u64 {
        Changeset::namespace(self)
    }

    fn changes(&self) -> impl Iterator<Item = ChangeRef<'_>> {
        Changeset::changes(self).iter().map(Change::as_ref)
    }
}
