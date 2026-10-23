/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::trust_marker::TrustMarker;

/// A plugin's claim of a sub-namespace under the reserved `dgraph.` prefix, so the predicates and types it owns can
/// be created at runtime via Alter even though they are not part of the pre-defined initial schema. Register one at
/// startup with [`register_reserved_namespace`](crate::register_reserved_namespace).
///
/// Ownership is deliberately explicit — a dynamic prefix and/or an exact name allowlist — so registering a namespace
/// cannot be used to admit arbitrary reserved predicates through alter or mutate. All names are bare (no namespace
/// prefix) and matched case-insensitively.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReservedNamespace {
    predicate_prefix: String,
    predicates: Vec<String>,
    types: Vec<String>,
    value_locked: Vec<String>,
    value_locked_prefixes: Vec<String>,
    trust_marker: Option<TrustMarker>,
}

impl ReservedNamespace {
    /// A namespace that claims nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Admits dynamically-named predicates by prefix (e.g. `dgraph.acme.rel.`). Empty means the namespace owns no
    /// dynamic predicates.
    pub fn with_predicate_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.predicate_prefix = prefix.into();
        self
    }

    /// The exact set of fixed predicate names the namespace owns.
    pub fn with_predicates<S: Into<String>>(
        mut self,
        predicates: impl IntoIterator<Item = S>,
    ) -> Self {
        self.predicates = predicates.into_iter().map(Into::into).collect();
        self
    }

    /// The exact set of type names the namespace owns.
    pub fn with_types<S: Into<String>>(mut self, types: impl IntoIterator<Item = S>) -> Self {
        self.types = types.into_iter().map(Into::into).collect();
        self
    }

    /// The owned predicates whose stored value only a request carrying the trust marker may write: authoritative
    /// state the owner must control.
    pub fn with_value_locked<S: Into<String>>(
        mut self,
        predicates: impl IntoIterator<Item = S>,
    ) -> Self {
        self.value_locked = predicates.into_iter().map(Into::into).collect();
        self
    }

    /// Locks predicates by prefix rather than by exact name, for a namespace whose owned predicates are created
    /// dynamically and so cannot be enumerated up front; typically the predicate prefix. Without it, predicates
    /// admitted by prefix are creatable via Alter but writable by anyone through mutate.
    pub fn with_value_locked_prefixes<S: Into<String>>(
        mut self,
        prefixes: impl IntoIterator<Item = S>,
    ) -> Self {
        self.value_locked_prefixes = prefixes.into_iter().map(Into::into).collect();
        self
    }

    /// The marker the owner's trusted in-process caller carries to write the value-locked predicates. Required when
    /// any value lock is set.
    pub fn with_trust_marker(mut self, marker: TrustMarker) -> Self {
        self.trust_marker = Some(marker);
        self
    }

    pub fn predicate_prefix(&self) -> &str {
        &self.predicate_prefix
    }

    pub fn predicates(&self) -> &[String] {
        &self.predicates
    }

    pub fn types(&self) -> &[String] {
        &self.types
    }

    pub fn value_locked(&self) -> &[String] {
        &self.value_locked
    }

    pub fn value_locked_prefixes(&self) -> &[String] {
        &self.value_locked_prefixes
    }

    pub fn trust_marker(&self) -> Option<TrustMarker> {
        self.trust_marker
    }
}
