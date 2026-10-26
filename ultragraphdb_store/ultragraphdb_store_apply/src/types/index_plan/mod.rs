/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_mapping::PredicateDeclaration;

/// What a change of a predicate's declaration does to its indexes: the decision table over its tokenizers, its count
/// index and its vector index. An index the new declaration names and the old does not is built; one the old names
/// and the new does not is removed; a vector index whose parameters change is built again. Reverse lists are always
/// kept, so no change builds them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct IndexPlan {
    pub build_tokenizers: Vec<String>,
    pub remove_tokenizers: Vec<String>,
    pub build_count: bool,
    pub remove_count: bool,
    pub build_vector: bool,
    pub remove_vector: bool,
}

impl IndexPlan {
    pub fn of(old: &PredicateDeclaration, new: &PredicateDeclaration) -> Self {
        let missing = |from: &PredicateDeclaration, to: &PredicateDeclaration| {
            from.tokenizers()
                .iter()
                .filter(|t| !to.tokenizers().contains(t))
                .cloned()
                .collect()
        };
        let vector_changes = old.vector_index() != new.vector_index();
        Self {
            build_tokenizers: missing(new, old),
            remove_tokenizers: missing(old, new),
            build_count: new.count_index() && !old.count_index(),
            remove_count: old.count_index() && !new.count_index(),
            build_vector: vector_changes && new.vector_index().is_some(),
            remove_vector: vector_changes && new.vector_index().is_none(),
        }
    }

    /// Whether the change builds an index.
    pub fn builds(&self) -> bool {
        !self.build_tokenizers.is_empty() || self.build_count || self.build_vector
    }

    /// The declaration queries read while the change's indexes are built: `new` without the indexes being built, and
    /// so without those being removed, which `new` does not name.
    pub fn interim(&self, new: &PredicateDeclaration) -> PredicateDeclaration {
        let mut interim = new.clone().with_tokenizers(
            new.tokenizers()
                .iter()
                .filter(|t| !self.build_tokenizers.contains(t))
                .cloned()
                .collect(),
        );
        interim = interim.with_count_index(new.count_index() && !self.build_count);
        if self.build_vector {
            interim = interim.without_vector_index();
        }
        interim
    }

    /// The declaration writes are applied under while the change's indexes are built: `new`'s checks, with the old and
    /// new declarations' tokenizers and count indexes both kept, and the vector index of `old` kept until the swap
    /// replaces it.
    pub fn maintained(
        &self,
        old: &PredicateDeclaration,
        new: &PredicateDeclaration,
    ) -> PredicateDeclaration {
        let mut tokenizers = new.tokenizers().to_vec();
        tokenizers.extend(self.remove_tokenizers.iter().cloned());
        let maintained = new
            .clone()
            .with_tokenizers(tokenizers)
            .with_count_index(new.count_index() || old.count_index());
        match (self.build_vector, old.vector_index()) {
            (true, Some(spec)) => maintained.with_vector_index(spec.clone()),
            (true, None) => maintained.without_vector_index(),
            (false, _) => maintained,
        }
    }
}
