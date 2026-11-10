/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod install_body_builder;

use crate::types::warning::Warning;

/// The sources of a tenant's schema, installed as a new catalog version built on `version` (`PROTOCOL.md` §8.4). The
/// client sends the sources with `epoch` zero and no report; the server answers with the version installed, the epoch
/// it takes effect at and the compilation report, or refuses with code 28.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `tenant` | u64 |
/// | 8 | `version` | u64: in a request, the version the sources build on, 0 for a tenant with none; in a reply, the version installed |
/// | 16 | `epoch` | u64: in a reply, the epoch the version takes effect at; 0 in a request |
/// | 24 | `staged` | u64: in a reply to sources holding a migration, the staged namespace's identifier; else 0 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | sources: ends |
/// | 1 | sources: text, for each source its name, then its text |
/// | 2 | the report, UTF-8; empty in a request |
/// | 3 | the migration report, UTF-8; empty in a request and without a migration |
/// | 4 | the warnings (`PROTOCOL.md` §4.9); empty in a request |
/// | 5 | the warnings' messages: ends |
/// | 6 | the warnings' messages: text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    tenant: u64,
    version: u64,
    epoch: u64,
    sources: Vec<(String, String)>,
    report: String,
    staged: u64,
    migration_report: String,
    warnings: Vec<Warning>,
}

impl Install {
    /// A request installing `sources`, each a name and its text, in `tenant`, built on the catalog version `version`.
    #[must_use]
    pub fn new(tenant: u64, version: u64, sources: Vec<(String, String)>) -> Self {
        Self {
            tenant,
            version,
            epoch: 0,
            sources,
            report: String::new(),
            staged: 0,
            migration_report: String::new(),
            warnings: Vec::new(),
        }
    }

    /// The reply: the catalog `version` installed, the `epoch` it takes effect at and the compilation `report`.
    #[must_use]
    pub fn installed(mut self, version: u64, epoch: u64, report: impl Into<String>) -> Self {
        self.version = version;
        self.epoch = epoch;
        self.report = report.into();
        self
    }

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

    /// The sources, each a name and its text.
    #[must_use]
    pub fn sources(&self) -> &[(String, String)] {
        &self.sources
    }

    /// The compilation report; empty in a request.
    #[must_use]
    pub fn report(&self) -> &str {
        &self.report
    }

    /// The reply to sources holding a migration: the staged namespace's identifier, at least 1.
    #[must_use]
    pub fn with_staged(self, staged: u64) -> Self {
        Self { staged, ..self }
    }

    /// In a reply to sources holding a migration, the staged namespace's identifier; else 0, and 0 in a request.
    #[must_use]
    pub fn staged(&self) -> u64 {
        self.staged
    }

    /// The reply with its migration report (`SCHEMA-EVOLUTION.md` section 5), the rename map among it.
    #[must_use]
    pub fn with_migration_report(self, report: impl Into<String>) -> Self {
        Self {
            migration_report: report.into(),
            ..self
        }
    }

    /// The migration report; empty in a request and without a migration.
    #[must_use]
    pub fn migration_report(&self) -> &str {
        &self.migration_report
    }

    /// The reply with the compilation's warnings, each `source` its source's place among the request's sources.
    #[must_use]
    pub fn with_warnings(self, warnings: Vec<Warning>) -> Self {
        Self { warnings, ..self }
    }

    /// The compilation's warnings; empty in a request.
    #[must_use]
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }
}
