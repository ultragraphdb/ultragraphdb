/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::sim_rng::SimRng;

/// The keyspace byte of the quad layout's forward lists.
pub const FORWARD: u8 = 0x10;
/// The keyspace byte of the quad layout's value index.
pub const VALUE_INDEX: u8 = 0x14;
/// The keyspace byte of the quad layout's statistics: a partition holds no catalog keys, so the one key per predicate
/// the workload writes, and the entry a drop writes once its prefix is empty, are a predicate's statistics.
pub const STATISTICS: u8 = 0x18;
/// The tokenizer byte of the value index's keys.
const TOKENIZER: u8 = 2;

const TENANTS: [u64; 3] = [0, 1, 5];
const GRAPHS: [u64; 2] = [0, 3];
/// The predicates `name` and `friend`, by UID.
pub const PREDICATES: [u64; 2] = [7, 9];
/// Index tokens, one a byte prefix of another and one holding a zero byte, which the stored key writes as a run code.
const TOKENS: [&[u8]; 4] = [b"ab", b"abc", b"a\x00b", b"b"];

/// The keys the store workload draws: forward lists, value index keys and statistics entries of a few tenants,
/// graphs and predicates, laid out as the quad layout lays them out (`[keyspace][tenant][fields]`, every number
/// a u64 big-endian).
#[derive(Debug, Clone, Default)]
pub struct KeySpace;

impl KeySpace {
    pub fn new() -> Self {
        Self
    }

    fn predicate_prefix(keyspace: u8, tenant: u64, graph: u64, predicate: u64) -> Vec<u8> {
        let mut key = vec![keyspace];
        key.extend_from_slice(&tenant.to_be_bytes());
        key.extend_from_slice(&graph.to_be_bytes());
        key.extend_from_slice(&predicate.to_be_bytes());
        key
    }

    pub fn key(&self, rng: &mut SimRng) -> Vec<u8> {
        let tenant = pick(rng, &TENANTS);
        let graph = pick(rng, &GRAPHS);
        let predicate = pick(rng, &PREDICATES);
        match rng.below(10) {
            0..6 => {
                let mut key = Self::predicate_prefix(FORWARD, tenant, graph, predicate);
                key.extend_from_slice(&rng.between(1, 6).to_be_bytes());
                key
            }
            6..9 => {
                let mut key = Self::predicate_prefix(VALUE_INDEX, tenant, graph, predicate);
                key.push(TOKENIZER);
                key.extend_from_slice(pick(rng, &TOKENS));
                key
            }
            _ => self.statistics_key(tenant, predicate),
        }
    }

    /// The key of predicate `predicate`'s statistics in `tenant`'s default graph. It draws nothing, as the catalog
    /// key it replaced drew nothing, so every seed's draws stay where they were.
    pub fn statistics_key(&self, tenant: u64, predicate: u64) -> Vec<u8> {
        Self::predicate_prefix(STATISTICS, tenant, 0, predicate)
    }

    /// A prefix to drop or scan: a tenant's keys of one keyspace, or one predicate's.
    pub fn prefix(&self, rng: &mut SimRng) -> Vec<u8> {
        let keyspace = pick(rng, &[FORWARD, VALUE_INDEX, STATISTICS]);
        let tenant = pick(rng, &TENANTS);
        let predicate = pick(rng, &PREDICATES);
        if rng.chance(500) {
            let mut prefix = vec![keyspace];
            prefix.extend_from_slice(&tenant.to_be_bytes());
            prefix
        } else if keyspace == STATISTICS {
            self.statistics_key(tenant, predicate)
        } else {
            let graph = pick(rng, &GRAPHS);
            Self::predicate_prefix(keyspace, tenant, graph, predicate)
        }
    }

    /// A tenant other than the root one, which is never banned.
    pub fn bannable(&self, rng: &mut SimRng) -> u64 {
        let others = TENANTS.get(1..).unwrap_or_default();
        pick(rng, others)
    }
}

/// The item of `items` at an index drawn from `rng`, one draw as indexing by `rng.index` takes; the default only for
/// an empty list, which no caller passes.
fn pick<T: Copy + Default>(rng: &mut SimRng, items: &[T]) -> T {
    items
        .get(rng.index(items.len()))
        .copied()
        .unwrap_or_default()
}
