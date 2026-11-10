/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod drop_command_view_body_view;

use ultragraphdb_core_ids::SessionToken;

use crate::types::drop_kind::DropKind;

/// The body of a received Drop frame, read in place; see [`crate::DropCommand`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropCommandView<'a> {
    kind: DropKind,
    tenant: u64,
    target: Option<&'a str>,
    session: [u8; 16],
    request: u64,
    commit_version: u64,
    token: Option<SessionToken>,
    catalog: u64,
}

impl<'a> DropCommandView<'a> {
    /// What the drop removes.
    pub fn kind(&self) -> DropKind {
        self.kind
    }

    /// The tenant the drop applies to.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The IRI of the predicate or graph dropped; `None` for the other kinds.
    pub fn target(&self) -> Option<&'a str> {
        self.target
    }

    /// The drop's session token; `None` in a request.
    #[must_use]
    pub fn token(&self) -> Option<&SessionToken> {
        self.token.as_ref()
    }

    /// The client's session; all zero for none.
    #[must_use]
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

    /// The catalog version a drop of kind 3 or 4 drops under; 0 for kinds 1 and 2.
    #[must_use]
    pub fn catalog(&self) -> u64 {
        self.catalog
    }
}
