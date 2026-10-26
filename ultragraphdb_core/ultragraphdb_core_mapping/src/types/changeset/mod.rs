/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::change::Change;

/// A write as the engines produce it and storage applies it: a namespace and its changes, applied in order, whole or
/// not at all. Its terms are RDF terms; storage resolves them to term IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Changeset {
    namespace: u64,
    changes: Vec<Change>,
}

impl Changeset {
    pub fn new(namespace: u64) -> Self {
        Self {
            namespace,
            changes: Vec::new(),
        }
    }

    pub fn push(&mut self, change: Change) {
        self.changes.push(change);
    }

    pub fn with(mut self, change: Change) -> Self {
        self.changes.push(change);
        self
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn into_changes(self) -> Vec<Change> {
        self.changes
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}
