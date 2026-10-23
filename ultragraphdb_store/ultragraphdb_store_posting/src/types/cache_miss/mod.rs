/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod cache_miss_drop;

use std::sync::Mutex;

use crate::types::list::List;
use crate::types::lists::state::State;

/// A read that missed the cache: the list it read from the store, and the fill it may make. The fill is the list
/// as of the applied timestamp when the read began, its `base`; [`crate::Lists::fill`] makes it only if no commit
/// wrote the key after `base` and the cache's epoch is still `epoch`. Dropping the miss without filling gives the
/// fill up.
#[derive(Debug)]
pub struct CacheMiss<'a> {
    pub(crate) state: &'a Mutex<State>,
    pub(crate) key: Vec<u8>,
    pub(crate) list: List,
    pub(crate) fill: List,
    pub(crate) base: u64,
    pub(crate) epoch: u64,
}

impl CacheMiss<'_> {
    /// The list at the read's timestamp.
    pub fn list(&self) -> &List {
        &self.list
    }

    /// The list at the read's timestamp, taken out of the miss, which gives the fill up.
    pub fn into_list(mut self) -> List {
        std::mem::replace(&mut self.list, List::empty(Vec::new()))
    }

    /// The applied timestamp when the read began: the fill is current from it.
    pub fn base(&self) -> u64 {
        self.base
    }

    /// The cache's epoch when the read began.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}
