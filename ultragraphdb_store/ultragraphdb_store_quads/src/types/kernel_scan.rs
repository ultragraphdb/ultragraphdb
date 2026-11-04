/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;

use ultragraphdb_core_keys::Keyspace;

/// A request of the read kernel ([`crate::QuadStore::scan`]): the lists of `keyspace` in `namespace` whose keys,
/// without their versions, lie from `from`, inclusive, to `to`, exclusive, or to the end of the namespace's keyspace,
/// read as of `version`, at most `limit` of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelScan {
    keyspace: Keyspace,
    namespace: u64,
    from: Vec<u8>,
    to: Option<Vec<u8>>,
    version: u64,
    limit: NonZeroUsize,
}

impl KernelScan {
    /// The scan of `keyspace` in `namespace` from `from` to the namespace's keyspace's end, as of `version`, at most
    /// `limit` lists.
    pub fn new(
        keyspace: Keyspace,
        namespace: u64,
        from: Vec<u8>,
        version: u64,
        limit: NonZeroUsize,
    ) -> Self {
        Self {
            keyspace,
            namespace,
            from,
            to: None,
            version,
            limit,
        }
    }

    /// The scan ending before `to`.
    pub fn to(mut self, to: Vec<u8>) -> Self {
        self.to = Some(to);
        self
    }

    /// The keyspace.
    pub fn keyspace(&self) -> Keyspace {
        self.keyspace
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The first key, inclusive.
    pub fn from(&self) -> &[u8] {
        &self.from
    }

    /// The key the scan ends before, if any.
    pub fn end(&self) -> Option<&[u8]> {
        self.to.as_deref()
    }

    /// The version the lists are read at.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The most lists the scan returns.
    pub fn limit(&self) -> NonZeroUsize {
        self.limit
    }
}
