/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::convert::Infallible;

use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion};

/// The catalog versions a partition fetched from the metadata group before its replay, which answer from here without a wait.
#[derive(Debug, Clone, Default)]
pub struct HeldCatalogs {
    versions: BTreeMap<(u64, u64), HeldVersion>,
}

impl HeldCatalogs {
    /// Holds `version`.
    pub fn hold(&mut self, version: HeldVersion) {
        let _replaced = self
            .versions
            .insert((version.tenant(), version.version()), version);
    }
}

impl CatalogSource for HeldCatalogs {
    type Error = Infallible;

    fn version_at(&self, tenant: u64, epoch: u64) -> Result<Option<HeldVersion>, Infallible> {
        Ok(self
            .versions
            .range((tenant, 0)..=(tenant, u64::MAX))
            .rev()
            .map(|(_, held)| held)
            .find(|held| held.effective() <= epoch)
            .cloned())
    }

    fn version(&self, tenant: u64, version: u64) -> Result<Option<HeldVersion>, Infallible> {
        Ok(self.versions.get(&(tenant, version)).cloned())
    }
}
