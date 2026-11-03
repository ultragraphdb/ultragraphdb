/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/index.go (LocalCache). Changed for ultragraphdb: translated to Rust; Go's
 * `error` as an associated type.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

/// The local cache of a persistent store.
pub trait LocalCache {
    /// Why a read failed.
    type Error;

    /// The value under `key`.
    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error>;

    /// The value under `key`, with the key's lock held.
    fn get_with_lock_held(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error>;

    /// The ID of the first entry under `prefix`, in key order, whose value passes `filter`; `None` when none passes,
    /// where Dgraph returns the uid 0.
    fn find(
        &self,
        prefix: &[u8],
        filter: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<SystemId>, Self::Error>;
}
