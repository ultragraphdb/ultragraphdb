/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod catalog_body_builder;

/// A namespace's catalog version from the metadata group (`PROTOCOL.md` §13.7): its number, the epoch it applies from and
/// its bytes, carried opaque. A body with `position` 0 is a request.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `version` | u64: the catalog version, 0 in a request for the current one; at least 1 in a reply |
/// | 16 | `epoch` | epoch: the epoch from which the version applies, 0 while it is pending; 0 in a request |
/// | 24 | `position` | u64: the metadata group's log position the version was read at, from 1; 0 in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the version's bytes; empty in a request |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    namespace: u64,
    version: u64,
    epoch: u64,
    position: u64,
    bytes: Vec<u8>,
}

impl Catalog {
    /// A request for `namespace`'s catalog `version`, 0 for its current one.
    pub fn request(namespace: u64, version: u64) -> Self {
        Self {
            namespace,
            version,
            epoch: 0,
            position: 0,
            bytes: Vec::new(),
        }
    }

    /// A reply: `namespace`'s catalog `version`, applying from `epoch` (0 while pending), read at `position`, holding
    /// `bytes`.
    pub fn reply(namespace: u64, version: u64, epoch: u64, position: u64, bytes: Vec<u8>) -> Self {
        Self {
            namespace,
            version,
            epoch,
            position,
            bytes,
        }
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The catalog version; 0 in a request for the current one.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The epoch from which the version applies; 0 while pending, and in a request.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The metadata group's log position; 0 in a request.
    pub fn position(&self) -> u64 {
        self.position
    }

    /// The version's bytes; empty in a request.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
