/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod declare_view_body_view;

use ultragraphdb_core_body::StringList;

use crate::types::declared::Declared;
use crate::types::drop_kind::DropKind;
use crate::utils::changes::{CHANGE_RECORD_SIZE, decode_declaration};
use crate::utils::whole_records::record_count;

/// The body of a received Declare frame, read in place; see [`crate::Declare`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclareView<'a> {
    tenant: u64,
    built_on: u64,
    principal: u64,
    version: u64,
    commit_version: u64,
    drop: Option<DropKind>,
    target: Option<&'a str>,
    records: &'a [u8],
    args: Vec<u32>,
    names: StringList<'a>,
}

impl<'a> DeclareView<'a> {
    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The catalog version the declarations are built on; 0 for a Drop.
    #[must_use]
    pub fn built_on(&self) -> u64 {
        self.built_on
    }

    /// The client's principal.
    #[must_use]
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The catalog version committed as pending; 0 in a request.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The first version of the effective epoch; 0 in a request and for a Drop.
    #[must_use]
    pub fn commit_version(&self) -> u64 {
        self.commit_version
    }

    /// Whether the body is a request: `version` and `commit_version` 0.
    #[must_use]
    pub fn is_request(&self) -> bool {
        self.version == 0 && self.commit_version == 0
    }

    /// The Drop's kind for `kind` 2; `None` for `kind` 1.
    #[must_use]
    pub fn drop_kind(&self) -> Option<DropKind> {
        self.drop
    }

    /// The IRI of the predicate or graph a Drop names, borrowed from the frame; `None` otherwise.
    #[must_use]
    pub fn target(&self) -> Option<&'a str> {
        self.target
    }

    /// The number of declarations: at least one for `kind` 1, none for a Drop.
    #[must_use]
    pub fn len(&self) -> usize {
        record_count::<CHANGE_RECORD_SIZE>(self.records)
    }

    /// Whether the body holds no declaration: a Drop.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Declaration `index`; `None` past the last.
    #[must_use]
    pub fn declaration(&self, index: usize) -> Option<Declared> {
        decode_declaration(self.records, index, &self.args, &self.names)
            .map(|(declaration, _)| declaration)
    }
}
