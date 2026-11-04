/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

/// An edge by its names: subject, predicate, object.
pub(crate) type Edge = (String, String, String);

/// The reference model: every edge's history as one partition holding every statement would keep it, each change at
/// its commit version; a reverse read at an epoch answers what this holds at that epoch's top.
#[derive(Debug, Default)]
pub(crate) struct EdgeModel {
    history: BTreeMap<Edge, BTreeMap<u64, bool>>,
}

impl EdgeModel {
    /// Records that `edge` is `held` from `version` on.
    pub(crate) fn record(&mut self, edge: Edge, version: u64, held: bool) {
        let _previous = self.history.entry(edge).or_default().insert(version, held);
    }

    /// Whether `edge` is held at `version`.
    pub(crate) fn held_at(&self, edge: &Edge, version: u64) -> bool {
        self.history
            .get(edge)
            .and_then(|changes| changes.range(..=version).next_back())
            .is_some_and(|(_, held)| *held)
    }

    /// The subjects holding `predicate` to `object` at `version`.
    pub(crate) fn subjects_at(
        &self,
        predicate: &str,
        object: &str,
        version: u64,
    ) -> BTreeSet<String> {
        self.history
            .keys()
            .filter(|(_, p, o)| p == predicate && o == object)
            .filter(|edge| self.held_at(edge, version))
            .map(|(s, _, _)| s.clone())
            .collect()
    }

    /// Every edge held now, with the version of the change that made it held.
    pub(crate) fn held(&self) -> impl Iterator<Item = (&Edge, u64)> {
        self.history.iter().filter_map(|(edge, changes)| {
            let (version, held) = changes.iter().next_back()?;
            held.then_some((edge, *version))
        })
    }
}
