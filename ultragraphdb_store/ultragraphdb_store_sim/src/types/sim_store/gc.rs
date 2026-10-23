/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Reverse;
use std::collections::BTreeSet;

use crate::types::sim_store::corrupted::Corrupted;
use crate::types::sim_store::state::{State, decode};

/// A version's discard flag, deletion and fence, when its bytes decode.
type Flags = (bool, bool, bool);

/// A full compaction under discard timestamp `discard`: of each key, removes the versions older than its newest
/// discard-flagged version or deletion at or below `discard`. Deletions are kept, and so are the versions a prefix
/// drop fenced and every version whose bytes cannot be decoded, as the compaction filter of `ultragraphdb_store_kv`
/// keeps them.
pub fn compact(state: &mut State, corrupted: &Corrupted, discard: u64) {
    let keys: BTreeSet<Vec<u8>> = state.data.keys().map(|(k, _)| k.clone()).collect();
    for key in keys {
        let versions: Vec<(u64, Option<Flags>)> = state
            .versions(&key)
            .map(|(v, bytes)| {
                let decoded = decode(&corrupted.bytes(&key, v, bytes))
                    .ok()
                    .map(|d| (d.discard, d.deletion, d.fenced));
                (v, decoded)
            })
            .collect();
        let cut = versions
            .iter()
            .find(|(v, d)| {
                *v <= discard && d.is_some_and(|(discard, deletion, _)| discard || deletion)
            })
            .map(|(v, _)| *v);
        if let Some(cut) = cut {
            versions
                .iter()
                .filter(|(v, d)| {
                    *v < cut && d.is_some_and(|(_, deletion, fenced)| !deletion && !fenced)
                })
                .for_each(|(v, _)| {
                    state.data.remove(&(key.clone(), Reverse(*v)));
                });
        }
    }
}

/// The sweep under tombstone horizon `horizon`: removes each deletion at or below it with no version of its key
/// below it.
pub fn sweep(state: &mut State, corrupted: &Corrupted, horizon: u64) {
    let swept: Vec<(Vec<u8>, u64)> = state
        .data
        .iter()
        .filter(|((key, Reverse(v)), bytes)| {
            *v <= horizon
                && decode(&corrupted.bytes(key, *v, bytes)).is_ok_and(|d| d.deletion)
                && state.versions(key).all(|(other, _)| other >= *v)
        })
        .map(|((key, Reverse(v)), _)| (key.clone(), *v))
        .collect();
    swept.into_iter().for_each(|(key, v)| {
        state.data.remove(&(key, Reverse(v)));
    });
}
