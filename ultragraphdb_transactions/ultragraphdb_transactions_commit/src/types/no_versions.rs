/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::convert::Infallible;

use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion};

/// The catalog source of a replay given none: it holds no version, so a replayed build's start record is refused as
/// naming a version the metadata group does not give.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NoVersions;

impl CatalogSource for NoVersions {
    type Error = Infallible;

    fn version_at(&self, _namespace: u64, _epoch: u64) -> Result<Option<HeldVersion>, Infallible> {
        Ok(None)
    }

    fn version(&self, _namespace: u64, _version: u64) -> Result<Option<HeldVersion>, Infallible> {
        Ok(None)
    }
}
