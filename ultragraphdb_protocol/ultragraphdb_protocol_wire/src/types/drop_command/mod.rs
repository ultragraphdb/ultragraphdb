/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod drop_command_body_builder;

use ultragraphdb_core_ids::SessionToken;

use crate::types::drop_kind::DropKind;

/// The Drop command's body: a predicate, a graph, a tenant's data, a tenant or every tenant dropped, on a
/// stream of its own and outside any transaction, committed through a session as a Commit is and answered with the
/// drop's commit version. Named for its command as [`crate::ErrorReply`] is, beside the standard `Drop` trait.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8, a [`DropKind`] |
/// | 8 | `tenant` | u64: the tenant of kinds 1 to 4; 0 for kind 5 |
/// | 16 | `session` | 16 bytes; all zero for none |
/// | 32 | `request` | u64; at least 1 with a session, 0 with none |
/// | 40 | `commit_version` | u64, 0 in a request |
/// | 48 | `token` | session token, 24 bytes; zero in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the term table's records: one IRI, the predicate's for kind 1 and the graph's for kind 2; none otherwise |
/// | 1 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropCommand {
    kind: DropKind,
    tenant: u64,
    target: Option<String>,
    session: [u8; 16],
    request: u64,
    commit_version: u64,
    token: Option<SessionToken>,
}

impl DropCommand {
    fn new(kind: DropKind, tenant: u64, target: Option<String>) -> Self {
        Self {
            kind,
            tenant,
            target,
            session: [0; 16],
            request: 0,
            commit_version: 0,
            token: None,
        }
    }

    /// A drop of the predicate `iri` of `tenant`.
    pub fn predicate(tenant: u64, iri: impl Into<String>) -> Self {
        Self::new(DropKind::Predicate, tenant, Some(iri.into()))
    }

    /// A drop of the graph `iri` of `tenant`.
    pub fn graph(tenant: u64, iri: impl Into<String>) -> Self {
        Self::new(DropKind::Graph, tenant, Some(iri.into()))
    }

    /// A drop of `tenant`'s data, keeping its catalog.
    pub fn tenant_data(tenant: u64) -> Self {
        Self::new(DropKind::TenantData, tenant, None)
    }

    /// A drop of `tenant`, its catalog with it.
    pub fn whole_tenant(tenant: u64) -> Self {
        Self::new(DropKind::Tenant, tenant, None)
    }

    /// A drop of every tenant.
    pub fn all() -> Self {
        Self::new(DropKind::All, 0, None)
    }

    /// The drop through `session` with the next `request` number of the session's counter, from 1; a session all zero
    /// commits without exactly-once, with `request` 0.
    pub fn with_session(mut self, session: [u8; 16], request: u64) -> Self {
        self.session = session;
        self.request = request;
        self
    }

    /// The reply: the drop's commit version.
    pub fn with_commit_version(mut self, commit_version: u64) -> Self {
        self.commit_version = commit_version;
        self
    }

    /// What the drop removes.
    pub fn kind(&self) -> DropKind {
        self.kind
    }

    /// The tenant the drop applies to.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The IRI of the predicate or graph dropped; `None` for the other kinds.
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// The reply's session token.
    pub fn with_token(mut self, token: SessionToken) -> Self {
        self.token = Some(token);
        self
    }

    /// The drop's session token; `None` in a request.
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    /// The client's session; all zero for none.
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The request's number within its session.
    pub fn request(&self) -> u64 {
        self.request
    }

    /// The commit version; zero in a request.
    pub fn commit_version(&self) -> u64 {
        self.commit_version
    }
}
