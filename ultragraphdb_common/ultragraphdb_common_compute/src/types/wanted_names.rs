/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

/// The dictionary keys a page's rows want, each once, in the order the rows name them.
#[derive(Debug, Default)]
pub(crate) struct WantedNames {
    keys: Vec<Vec<u8>>,
    seen: HashSet<Vec<u8>>,
}

impl WantedNames {
    /// Wants `key`, unless it is wanted already.
    pub(crate) fn want(&mut self, key: Vec<u8>) {
        if self.seen.insert(key.clone()) {
            self.keys.push(key);
        }
    }

    /// How many keys are wanted.
    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether no key is wanted.
    pub(crate) fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The keys, in the order first wanted.
    pub(crate) fn keys(&self) -> &[Vec<u8>] {
        &self.keys
    }
}
