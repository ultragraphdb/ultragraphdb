/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (List). Changed for ultragraphdb: a Rust type in place of the protobuf
 * message.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// A plain UID list, sorted where the set operations need it: Dgraph's `pb.List`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct List {
    uids: Vec<u64>,
}

impl List {
    /// A list of `uids`.
    pub fn new(uids: Vec<u64>) -> Self {
        Self { uids }
    }

    /// The UIDs.
    pub fn uids(&self) -> &[u64] {
        &self.uids
    }

    /// The UIDs, for the set operations that write their result in place.
    pub fn uids_mut(&mut self) -> &mut Vec<u64> {
        &mut self.uids
    }

    /// The UIDs, taken out of the list.
    pub fn into_uids(self) -> Vec<u64> {
        self.uids
    }
}
