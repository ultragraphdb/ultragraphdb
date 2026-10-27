/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (ListOptions). Changed for ultragraphdb: translated to Rust, built
 * from its read timestamp.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::List as UidList;

/// What [`crate::List::uids`] returns: the uids at the read timestamp above the after-uid, intersected with a list
/// when given, the first `first` of them when `first` is positive. Dgraph's `ListOptions`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ListOptions {
    read_ts: u64,
    after_uid: u64,
    intersect: Option<UidList>,
    first: i64,
}

impl ListOptions {
    /// Every uid at `read_ts`.
    pub fn at(read_ts: u64) -> Self {
        Self {
            read_ts,
            ..Self::default()
        }
    }

    /// Only the uids above `after_uid`.
    pub fn after(mut self, after_uid: u64) -> Self {
        self.after_uid = after_uid;
        self
    }

    /// Only the uids also in `uids`, a sorted list.
    pub fn intersect(mut self, uids: UidList) -> Self {
        self.intersect = Some(uids);
        self
    }

    /// The first `first` uids when positive; zero and negative counts bound nothing.
    pub fn first(mut self, first: i64) -> Self {
        self.first = first;
        self
    }

    pub fn read_ts(&self) -> u64 {
        self.read_ts
    }

    pub fn after_uid(&self) -> u64 {
        self.after_uid
    }

    pub fn intersect_with(&self) -> Option<&UidList> {
        self.intersect.as_ref()
    }

    pub fn first_count(&self) -> i64 {
        self.first
    }
}
