/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_proto_pb::pb::SchemaUpdate;

/// How many fences are kept while no request is open to bound them.
const FENCES_KEPT: usize = 1024;

/// A drop or a schema change: a request that read below `ts` and writes a key under one of `prefixes` is refused at
/// commit.
#[derive(Debug, Clone)]
pub(crate) struct Fence {
    pub ts: u64,
    pub prefixes: Vec<Vec<u8>>,
}

impl Fence {
    /// Whether the fence refuses a request reading at `read_ts` that writes `key`.
    pub fn refuses(&self, read_ts: u64, key: &[u8]) -> bool {
        read_ts < self.ts && self.prefixes.iter().any(|p| key.starts_with(p))
    }
}

/// A predicate whose indexes are being built: its schema before and after the change.
#[derive(Debug, Clone)]
pub(crate) struct Rebuilding {
    pub id: u64,
    pub old: SchemaUpdate,
    pub new: SchemaUpdate,
}

/// What the applier keeps between its operations: the fences of drops and schema changes, and the rebuilds under
/// way.
#[derive(Debug, Default)]
pub(crate) struct ApplyState {
    pub fences: Vec<Fence>,
    pub rebuilds: BTreeMap<String, Rebuilding>,
    pub next_job: u64,
}

impl ApplyState {
    /// Adds a fence, dropping those no open request can be refused by: the fences at or below the oldest open read,
    /// or, while none is open, all but the newest.
    pub fn fence(&mut self, fence: Fence, oldest_open_read: Option<u64>) {
        match oldest_open_read {
            Some(oldest) => self.fences.retain(|f| f.ts > oldest),
            None => {
                let excess = self.fences.len().saturating_sub(FENCES_KEPT - 1);
                self.fences.drain(..excess);
            }
        }
        self.fences.push(fence);
    }
}
