/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod members_body_builder;

use crate::types::role_kind::RoleKind;

/// The roles that joined (`PROTOCOL.md` §13.2): per storage partition, the role address of the process hosting it, and
/// each compute process's address. The requester sends no member. The command header is all zero.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | members, 12 bytes each ([`crate::Member`]); empty in a request |
/// | 1 | addresses: string list `ends` |
/// | 2 | addresses: string list `text` |
///
/// Members come compute first, by their addresses' UTF-8 bytes, then storage by partition; the builder writes them in
/// the order pushed and interns the addresses in member order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Members {
    members: Vec<(RoleKind, u32, String)>,
}

impl Members {
    /// A request, or a reply with no member yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A compute process at `address`, appended.
    pub fn push_compute(&mut self, address: impl Into<String>) {
        self.members.push((RoleKind::Compute, 0, address.into()));
    }

    /// Storage `partition`, hosted by the process at `address`, appended.
    pub fn push_storage(&mut self, partition: u32, address: impl Into<String>) {
        self.members
            .push((RoleKind::Storage, partition, address.into()));
    }

    /// The number of members.
    #[must_use]
    pub fn len(&self) -> usize {
        self.members.len()
    }

    /// Whether there is no member: a request, or an empty reply.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Member `index`: its kind, its partition (0 for compute) and its address; `None` past the last.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<(RoleKind, u32, &str)> {
        self.members
            .get(index)
            .map(|(kind, partition, address)| (*kind, *partition, address.as_str()))
    }
}
