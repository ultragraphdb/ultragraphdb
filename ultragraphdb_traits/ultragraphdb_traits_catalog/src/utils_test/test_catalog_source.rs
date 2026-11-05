/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::traits::catalog_source::CatalogSource;
use crate::types::default_entry::DefaultEntry;
use crate::types::held_version::HeldVersion;
use crate::utils_test::catalog_unreachable::CatalogUnreachable;

/// A [`CatalogSource`] for tests, standing in for the metadata group: the versions committed to it, each with its
/// effective epoch, and the default-entry records sent to it, keyed by (tenant, predicate), a record that exists
/// found and left as it is. It answers nothing while unreachable. Its clones share one state, as the partitions of one
/// cluster share one metadata group, so a test keeps a handle on what its partitions read and record.
#[derive(Debug, Clone, Default)]
pub struct TestCatalogSource {
    // Shared: the clones a test hands its partitions read one metadata group, and the test inspects and steers it.
    state: Arc<Mutex<State>>,
}

/// What a [`TestCatalogSource`]'s clones share.
#[derive(Debug, Default)]
struct State {
    versions: BTreeMap<(u64, u64), HeldVersion>,
    defaults: BTreeMap<(u64, String), DefaultEntry>,
    unreachable: bool,
}

impl TestCatalogSource {
    /// A reachable source holding no version and no default entry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds `version`, replacing a version of its tenant and number.
    pub fn commit(&self, version: HeldVersion) {
        let key = (version.tenant(), version.version());
        let _replaced: Option<HeldVersion> = self.lock().versions.insert(key, version);
    }

    /// Records `entry` as the group's command does: `true` when it created the record, `false` when it found one of
    /// the entry's tenant and predicate, which it leaves as it was.
    ///
    /// # Errors
    ///
    /// [`CatalogUnreachable`] while the source is unreachable, recording nothing.
    pub fn record_default(&self, entry: &DefaultEntry) -> Result<bool, CatalogUnreachable> {
        let mut state = self.lock();
        if state.unreachable {
            return Err(CatalogUnreachable);
        }
        let key = (entry.tenant(), entry.predicate().to_string());
        if state.defaults.contains_key(&key) {
            return Ok(false);
        }
        let _created: Option<DefaultEntry> = state.defaults.insert(key, entry.clone());
        Ok(true)
    }

    /// Every default-entry record of `tenant`, by predicate.
    #[must_use]
    pub fn defaults(&self, tenant: u64) -> Vec<DefaultEntry> {
        self.lock()
            .defaults
            .values()
            .filter(|e| e.tenant() == tenant)
            .cloned()
            .collect()
    }

    /// Makes the source reachable or not; an unreachable source answers and records nothing.
    pub fn set_reachable(&self, reachable: bool) {
        self.lock().unreachable = !reachable;
    }

    /// The shared state, a poisoned lock's included: every change to it is made whole before the guard drops.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl CatalogSource for TestCatalogSource {
    type Error = CatalogUnreachable;

    fn version_at(&self, tenant: u64, epoch: u64) -> Result<Option<HeldVersion>, Self::Error> {
        let state = self.lock();
        if state.unreachable {
            return Err(CatalogUnreachable);
        }
        // The versions of one tenant in key order are in number order, and a later version takes effect at or
        // after an earlier one, so the last at or below the epoch, found from the end, is the one in effect.
        Ok(state
            .versions
            .range((tenant, 0)..=(tenant, u64::MAX))
            .map(|(_, v)| v)
            .rev()
            .find(|v| v.effective() <= epoch)
            .cloned())
    }

    fn version(&self, tenant: u64, version: u64) -> Result<Option<HeldVersion>, Self::Error> {
        let state = self.lock();
        if state.unreachable {
            return Err(CatalogUnreachable);
        }
        Ok(state.versions.get(&(tenant, version)).cloned())
    }
}
