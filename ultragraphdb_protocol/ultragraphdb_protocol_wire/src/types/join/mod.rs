/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod join_body_builder;

use crate::types::role_kind::RoleKind;

/// A storage or compute process registers with the frontend (`PROTOCOL.md` §13.1): its kind, its role address and, for
/// storage, the partitions it hosts. The frontend answers with the same fields.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8: 2 compute, 3 storage |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the role address, UTF-8: `tcp://host:port` or `unix:///path` |
/// | 1 | the partitions it hosts, u32s, ascending; empty for compute |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Join {
    kind: RoleKind,
    address: String,
    partitions: Vec<u32>,
}

impl Join {
    /// A compute process joining from `address`.
    pub fn compute(address: impl Into<String>) -> Self {
        Self {
            kind: RoleKind::Compute,
            address: address.into(),
            partitions: Vec::new(),
        }
    }

    /// A storage process joining from `address`, hosting `partitions`, ascending.
    pub fn storage(address: impl Into<String>, partitions: Vec<u32>) -> Self {
        Self {
            kind: RoleKind::Storage,
            address: address.into(),
            partitions,
        }
    }

    /// The role's kind: compute or storage.
    pub fn kind(&self) -> RoleKind {
        self.kind
    }

    /// The role's role address.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The partitions it hosts; none for compute.
    pub fn partitions(&self) -> &[u32] {
        &self.partitions
    }
}
