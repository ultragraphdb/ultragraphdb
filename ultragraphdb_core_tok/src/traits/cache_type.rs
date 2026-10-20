/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/index/index.go (CacheType). Changed for ultragraphdb: translated to Rust; Go's
 * `error` as an associated type.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The cache of a persistent store a vector index reads through.
pub trait CacheType {
    /// Why a read failed.
    type Error;

    /// The value under `key`.
    fn get(&self, key: &[u8]) -> Result<Vec<u8>, Self::Error>;

    /// The read timestamp.
    fn ts(&self) -> u64;

    /// The first uid under `prefix` whose value passes `filter`.
    fn find(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, Self::Error>;
}
