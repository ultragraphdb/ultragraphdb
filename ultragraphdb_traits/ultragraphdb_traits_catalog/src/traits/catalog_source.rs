/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::held_version::HeldVersion;

/// The source a partition reads its tenants' catalog versions from: the metadata group in a cluster.
///
/// A version takes effect at the epoch the sequencer assigned it, and the source answers only versions it has given an
/// effective epoch: a pending one is no partition's yet. It retains every version a partition's untrimmed log may
/// name. Both reads return at once; a source fetching over the network answers from what it already holds.
pub trait CatalogSource {
    /// Why the source cannot answer now: unreachable, or failing.
    type Error: std::error::Error;

    /// The version of `tenant`'s catalog in effect at `epoch`: the one of the highest effective epoch at or below
    /// it, `None` when no version takes effect by then, the tenant then holding its initial catalog.
    ///
    /// # Errors
    ///
    /// The source's error when it cannot answer now.
    fn version_at(&self, tenant: u64, epoch: u64) -> Result<Option<HeldVersion>, Self::Error>;

    /// Version `version` of `tenant`'s catalog, as a replayed record naming it asks: `None` when the source holds
    /// no such version with an effective epoch.
    ///
    /// # Errors
    ///
    /// The source's error when it cannot answer now.
    fn version(&self, tenant: u64, version: u64) -> Result<Option<HeldVersion>, Self::Error>;
}
