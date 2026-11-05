/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod data_dictionary_describe;

use std::sync::{Arc, Mutex};

use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::quad_error::QuadError;
use crate::types::quad_store::QuadStore;
use crate::types::shapes::Shapes;
use crate::utils::lock::lock;

/// The data dictionary of one tenant: one read API, independent of any query language, answering what the data
/// holds at a snapshot. It keeps the shapes of the last pass, which [`DataDictionary::run_pass`] computes over a
/// snapshot, as a background task runs it, and replaces whole; no write reads or updates them.
#[derive(Debug)]
pub struct DataDictionary {
    tenant: u64,
    // Shared and locked: a background pass replaces the shapes whole while readers hold the previous ones.
    shapes: Mutex<Option<Arc<Shapes>>>,
}

impl DataDictionary {
    /// The data dictionary of `tenant`, with no shapes before its first pass.
    pub fn new(tenant: u64) -> Self {
        Self {
            tenant,
            shapes: Mutex::new(None),
        }
    }

    /// The tenant it describes.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The shapes of the last pass, if one completed.
    pub fn shapes(&self) -> Option<Arc<Shapes>> {
        lock(&self.shapes).clone()
    }

    /// Computes the shapes at `read_ts` and, unless a pass over a newer snapshot completed meanwhile, replaces the
    /// shapes held with them.
    pub fn run_pass<S, C, B>(
        &self,
        quads: &QuadStore<S, C, B>,
        read_ts: u64,
    ) -> Result<Arc<Shapes>, QuadError>
    where
        S: Store,
        C: ListCache<Value = CachedList>,
        B: BlockSource,
    {
        let shapes = Arc::new(Shapes::compute(quads, self.tenant, read_ts)?);
        let mut held = lock(&self.shapes);
        if held.as_ref().is_none_or(|h| h.read_ts() <= read_ts) {
            *held = Some(shapes.clone());
        }
        Ok(shapes)
    }
}
