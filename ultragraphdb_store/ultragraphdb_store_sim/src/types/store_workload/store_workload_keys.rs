/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::sim_rng::SimRng;

/// The keyspace byte of the quad layout's forward lists.
pub const FORWARD: u8 = 0x10;
/// The keyspace byte of the quad layout's value index.
pub const VALUE_INDEX: u8 = 0x14;
/// The keyspace byte of the quad layout's catalog.
pub const CATALOG: u8 = 0x01;
/// The catalog's entry kind of a predicate declaration.
const PREDICATE: u8 = 1;
/// The tokenizer byte of the value index's keys.
const TOKENIZER: u8 = 2;

const NAMESPACES: [u64; 3] = [0, 1, 5];
const GRAPHS: [u64; 2] = [0, 3];
/// The predicates `name` and `friend`, by UID.
pub const PREDICATES: [u64; 2] = [7, 9];
/// Index tokens, one a byte prefix of another and one holding a zero byte, which the stored key escapes.
const TOKENS: [&[u8]; 4] = [b"ab", b"abc", b"a\x00b", b"b"];

/// The keys the store workload draws: forward lists, value index keys and catalog entries of a few namespaces,
/// graphs and predicates, laid out as the quad layout lays them out (`[keyspace][namespace][fields]`, every number
/// a u64 big-endian).
#[derive(Debug, Clone, Default)]
pub struct KeySpace;

impl KeySpace {
    pub fn new() -> Self {
        Self
    }

    fn predicate_prefix(keyspace: u8, namespace: u64, graph: u64, predicate: u64) -> Vec<u8> {
        let mut key = vec![keyspace];
        key.extend_from_slice(&namespace.to_be_bytes());
        key.extend_from_slice(&graph.to_be_bytes());
        key.extend_from_slice(&predicate.to_be_bytes());
        key
    }

    pub fn key(&self, rng: &mut SimRng) -> Vec<u8> {
        let namespace = NAMESPACES[rng.index(NAMESPACES.len())];
        let graph = GRAPHS[rng.index(GRAPHS.len())];
        let predicate = PREDICATES[rng.index(PREDICATES.len())];
        match rng.below(10) {
            0..6 => {
                let mut key = Self::predicate_prefix(FORWARD, namespace, graph, predicate);
                key.extend_from_slice(&rng.between(1, 6).to_be_bytes());
                key
            }
            6..9 => {
                let mut key = Self::predicate_prefix(VALUE_INDEX, namespace, graph, predicate);
                key.push(TOKENIZER);
                key.extend_from_slice(TOKENS[rng.index(TOKENS.len())]);
                key
            }
            _ => self.catalog_key(namespace, predicate),
        }
    }

    /// The catalog key of predicate `predicate`'s declaration in `namespace`.
    pub fn catalog_key(&self, namespace: u64, predicate: u64) -> Vec<u8> {
        let mut key = vec![CATALOG];
        key.extend_from_slice(&namespace.to_be_bytes());
        key.push(PREDICATE);
        key.extend_from_slice(&predicate.to_be_bytes());
        key
    }

    /// A prefix to drop or scan: a namespace's keys of one keyspace, or one predicate's.
    pub fn prefix(&self, rng: &mut SimRng) -> Vec<u8> {
        let keyspaces = [FORWARD, VALUE_INDEX, CATALOG];
        let keyspace = keyspaces[rng.index(keyspaces.len())];
        let namespace = NAMESPACES[rng.index(NAMESPACES.len())];
        let predicate = PREDICATES[rng.index(PREDICATES.len())];
        if rng.chance(500) {
            let mut prefix = vec![keyspace];
            prefix.extend_from_slice(&namespace.to_be_bytes());
            prefix
        } else if keyspace == CATALOG {
            self.catalog_key(namespace, predicate)
        } else {
            let graph = GRAPHS[rng.index(GRAPHS.len())];
            Self::predicate_prefix(keyspace, namespace, graph, predicate)
        }
    }

    /// A namespace other than the root one, which is never banned.
    pub fn bannable(&self, rng: &mut SimRng) -> u64 {
        NAMESPACES[1 + rng.index(NAMESPACES.len() - 1)]
    }
}
