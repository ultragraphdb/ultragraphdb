/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock};

use ultragraphdb_proto_pb::pb::PostingList;

/// The deltas a batch's requests have stacked, by key, each with the number of the request that pushed it.
#[derive(Debug, Default)]
struct Stack {
    layers: BTreeMap<Vec<u8>, Vec<(u64, Arc<PostingList>)>>,
    pushed: u64,
}

/// A view of one batch's stacked layers: the batch holds the stack and appends each pushed request's deltas to it,
/// and a request begun from the batch holds a view that sees the layers of the requests pushed before it began and
/// none pushed after. Views share the one stack, so beginning a request costs the same whatever the batch holds,
/// where a copy of the batch's map per request made a batch of n requests cost O(n²) map entries.
#[derive(Debug, Clone, Default)]
pub(crate) struct StackedLayers {
    stack: Arc<RwLock<Stack>>,
    upto: u64,
}

impl StackedLayers {
    /// A view of the layers pushed so far.
    pub fn view(&self) -> Self {
        let upto = self.read(|s| s.pushed);
        Self {
            stack: Arc::clone(&self.stack),
            upto,
        }
    }

    /// The layers of `key` this view sees, oldest first.
    pub fn get(&self, key: &[u8]) -> Vec<Arc<PostingList>> {
        self.read(|s| {
            s.layers.get(key).map_or_else(Vec::new, |layers| {
                layers
                    .iter()
                    .take_while(|(request, _)| *request < self.upto)
                    .map(|(_, delta)| Arc::clone(delta))
                    .collect()
            })
        })
    }

    /// Stacks the deltas of the next request, seen by the views taken after it.
    pub fn push(&self, deltas: Vec<(Vec<u8>, PostingList)>) {
        let mut stack = self.stack.write().unwrap_or_else(PoisonError::into_inner);
        let request = stack.pushed;
        for (key, delta) in deltas {
            stack
                .layers
                .entry(key)
                .or_default()
                .push((request, Arc::new(delta)));
        }
        stack.pushed += 1;
    }

    fn read<T>(&self, f: impl FnOnce(&Stack) -> T) -> T {
        f(&self.stack.read().unwrap_or_else(PoisonError::into_inner))
    }
}
