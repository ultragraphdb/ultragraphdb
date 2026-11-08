/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod install_view_body_view;

use crate::types::string_list::StringList;

/// The body of a received Install frame, read in place; see [`crate::Install`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallView<'a> {
    tenant: u64,
    version: u64,
    epoch: u64,
    sources: StringList<'a>,
    report: &'a str,
}

impl<'a> InstallView<'a> {
    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// In a request, the version the sources build on; in a reply, the version installed.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// In a reply, the epoch the version takes effect at; 0 in a request.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The number of sources, at least one.
    #[must_use]
    pub fn source_count(&self) -> usize {
        self.sources.len() / 2
    }

    /// Source `index`: its name and its text; `None` past the last.
    #[must_use]
    pub fn source(&self, index: usize) -> Option<(&'a str, &'a str)> {
        let at = index.checked_mul(2)?;
        Some((self.sources.get(at)?, self.sources.get(at.checked_add(1)?)?))
    }

    /// The compilation report; empty in a request.
    #[must_use]
    pub fn report(&self) -> &'a str {
        self.report
    }
}
