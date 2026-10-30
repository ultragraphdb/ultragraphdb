/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod drop_command_body_builder;

use crate::types::drop_kind::DropKind;

/// The Drop command's body: a predicate, a graph, a namespace's data, a namespace or every namespace dropped, on a
/// stream of its own and outside any transaction, committed through a session as a Commit is and answered with the
/// drop's commit version. Named for its command as [`crate::ErrorReply`] is, beside the standard `Drop` trait.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8, a [`DropKind`] |
/// | 8 | `namespace` | u64: the namespace of kinds 1 to 4; 0 for kind 5 |
/// | 16 | `session` | u64 |
/// | 24 | `request` | u64 |
/// | 32 | `commit_version` | u64, 0 in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the term table's records: one IRI, the predicate's for kind 1 and the graph's for kind 2; none otherwise |
/// | 1 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropCommand {
    kind: DropKind,
    namespace: u64,
    target: Option<String>,
    session: u64,
    request: u64,
    commit_version: u64,
}

impl DropCommand {
    fn new(kind: DropKind, namespace: u64, target: Option<String>) -> Self {
        Self {
            kind,
            namespace,
            target,
            session: 0,
            request: 0,
            commit_version: 0,
        }
    }

    /// A drop of the predicate `iri` of `namespace`.
    pub fn predicate(namespace: u64, iri: impl Into<String>) -> Self {
        Self::new(DropKind::Predicate, namespace, Some(iri.into()))
    }

    /// A drop of the graph `iri` of `namespace`.
    pub fn graph(namespace: u64, iri: impl Into<String>) -> Self {
        Self::new(DropKind::Graph, namespace, Some(iri.into()))
    }

    /// A drop of `namespace`'s data, keeping its catalog.
    pub fn namespace_data(namespace: u64) -> Self {
        Self::new(DropKind::NamespaceData, namespace, None)
    }

    /// A drop of `namespace`, its catalog with it.
    pub fn whole_namespace(namespace: u64) -> Self {
        Self::new(DropKind::Namespace, namespace, None)
    }

    /// A drop of every namespace.
    pub fn all() -> Self {
        Self::new(DropKind::All, 0, None)
    }

    /// The drop through `session` with the session's next `request` number; session 0 commits without exactly-once.
    pub fn with_session(mut self, session: u64, request: u64) -> Self {
        self.session = session;
        self.request = request;
        self
    }

    /// The reply: the drop's commit version.
    pub fn with_commit_version(mut self, commit_version: u64) -> Self {
        self.commit_version = commit_version;
        self
    }

    pub fn kind(&self) -> DropKind {
        self.kind
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The IRI of the predicate or graph dropped; `None` for the other kinds.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn request(&self) -> u64 {
        self.request
    }

    pub fn commit_version(&self) -> u64 {
        self.commit_version
    }
}
