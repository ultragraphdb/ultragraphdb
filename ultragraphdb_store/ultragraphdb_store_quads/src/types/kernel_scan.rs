/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;

use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_traits_store::Scan;

use crate::errors::quad_error::QuadError;

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

    /// Refuses a keyspace holding no lists.
    ///
    /// # Errors
    ///
    /// [`QuadError::corrupted`] naming the keyspace's prefix.
    pub fn check_keyspace(&self) -> Result<(), QuadError> {
        if self.keyspace.is_list() {
            return Ok(());
        }
        Err(QuadError::corrupted(
            &key_prefix(self.keyspace, false, self.namespace, None, None),
            "the read kernel reads lists, and the keyspace holds none",
        ))
    }

    /// The range within the namespace's keyspace, a bound outside it clamped to it: its first key, inclusive, and the key it
    /// ends before.
    pub fn bounds(&self) -> (Vec<u8>, Option<Vec<u8>>) {
        let whole = Scan::prefix(key_prefix(self.keyspace, false, self.namespace, None, None));
        let start = whole.start().max(self.from()).to_vec();
        let end = match (whole.end(), self.end()) {
            (Some(a), Some(b)) => Some(a.min(b).to_vec()),
            (a, b) => a.or(b).map(<[u8]>::to_vec),
        };
        (start, end)
    }
}
