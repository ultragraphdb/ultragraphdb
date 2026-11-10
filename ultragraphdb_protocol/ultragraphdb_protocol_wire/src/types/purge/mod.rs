/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod purge_body_builder;

/// The legal purge of instances a list of IRIs names, removed with every version of their data (`PROTOCOL.md` §8.5).
/// The client sends the tenant and the IRIs with `epoch` and `removed` 0; the server answers with the purge epoch and
/// the instances removed. Refused with code 7 before stage 8.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `tenant` | u64 |
/// | 8 | `epoch` | epoch: in a reply, the purge epoch; 0 in a request |
/// | 16 | `removed` | u32: in a reply, the instances removed; 0 in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the term table's records: the instances' IRIs, each of kind 1, 1 to 10,000 |
/// | 1 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Purge {
    tenant: u64,
    epoch: u64,
    removed: u32,
    iris: Vec<String>,
}

impl Purge {
    /// A request purging the instances `iris` name in `tenant`.
    #[must_use]
    pub fn new(tenant: u64, iris: Vec<String>) -> Self {
        Self {
            tenant,
            epoch: 0,
            removed: 0,
            iris,
        }
    }

    /// The reply: the purge `epoch` and the instances `removed`.
    #[must_use]
    pub fn purged(self, epoch: u64, removed: u32) -> Self {
        Self {
            epoch,
            removed,
            ..self
        }
    }

    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// In a reply, the purge epoch; 0 in a request.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// In a reply, the instances removed; 0 in a request.
    #[must_use]
    pub fn removed(&self) -> u32 {
        self.removed
    }

    /// The instances' IRIs.
    #[must_use]
    pub fn iris(&self) -> &[String] {
        &self.iris
    }
}
