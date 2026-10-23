/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_traits_store::Item;

/// The items of a scan of a [`crate::SimSnapshot`], read when the scan was opened; a failure is its last item.
#[derive(Debug, Clone)]
pub struct SimScan {
    items: VecDeque<Result<Item, StoreError>>,
}

impl SimScan {
    pub(crate) fn new(items: Vec<Result<Item, StoreError>>) -> Self {
        Self {
            items: items.into(),
        }
    }
}

impl Iterator for SimScan {
    type Item = Result<Item, StoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.items.pop_front()
    }
}
