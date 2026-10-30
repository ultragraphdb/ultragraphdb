/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod drop_command_view_body_view;

use crate::types::drop_kind::DropKind;

/// The body of a received Drop frame, read in place; see [`crate::DropCommand`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DropCommandView<'a> {
    kind: DropKind,
    namespace: u64,
    target: Option<&'a str>,
    session: u64,
    request: u64,
    commit_version: u64,
}

impl<'a> DropCommandView<'a> {
    pub fn kind(&self) -> DropKind {
        self.kind
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The IRI of the predicate or graph dropped; `None` for the other kinds.
    pub fn target(&self) -> Option<&'a str> {
        self.target
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
