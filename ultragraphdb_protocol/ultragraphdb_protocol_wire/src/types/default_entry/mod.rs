/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod default_entry_body_builder;

/// A partition whose commit first wrote a predicate the catalog version in effect does not declare asks the metadata
/// group to record the predicate's default entry (`PROTOCOL.md` §13.18); the frontend answers once the record is
/// durable, with every field as sent and `found` 1 when the record already existed.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `found` | u8 boolean: 0 in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the term table's records: one IRI, the predicate's |
/// | 1 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DefaultEntry {
    namespace: u64,
    found: bool,
    predicate: String,
}

impl DefaultEntry {
    /// The request recording the default entry of `predicate` in `namespace`.
    pub fn request(namespace: u64, predicate: impl Into<String>) -> Self {
        Self {
            namespace,
            found: false,
            predicate: predicate.into(),
        }
    }

    /// The reply: `found` whether the record already existed.
    pub fn with_found(mut self, found: bool) -> Self {
        self.found = found;
        self
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// Whether the record already existed; false in a request.
    pub fn found(&self) -> bool {
        self.found
    }

    /// The predicate's IRI.
    pub fn predicate(&self) -> &str {
        &self.predicate
    }
}
