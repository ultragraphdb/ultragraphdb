/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::sim_rng::SimRng;

/// The prefix byte of Dgraph's data keys.
pub const PREFIX_DATA: u8 = 0x00;
/// The prefix byte of Dgraph's index keys.
pub const PREFIX_INDEX: u8 = 0x02;
/// The prefix byte of Dgraph's schema keys.
pub const PREFIX_SCHEMA: u8 = 0x01;

const NAMESPACES: [u64; 3] = [0, 1, 5];
const ATTRS: [&str; 2] = ["name", "friend"];
/// Index terms, one a byte prefix of another and one holding a zero byte, which the stored key escapes.
const TERMS: [&[u8]; 4] = [b"ab", b"abc", b"a\x00b", b"b"];

/// The keys the store workload draws: data, index and schema keys of a few namespaces and predicates, laid out as
/// Dgraph lays them out (`[prefix][namespace][attribute length][attribute][uid or term]`).
#[derive(Debug, Clone, Default)]
pub struct KeySpace;

impl KeySpace {
    pub fn new() -> Self {
        Self
    }

    fn attr_prefix(prefix: u8, namespace: u64, attr: &str) -> Vec<u8> {
        let mut key = vec![prefix];
        key.extend_from_slice(&namespace.to_be_bytes());
        key.extend_from_slice(&(attr.len() as u16).to_be_bytes());
        key.extend_from_slice(attr.as_bytes());
        key
    }

    pub fn key(&self, rng: &mut SimRng) -> Vec<u8> {
        let namespace = NAMESPACES[rng.index(NAMESPACES.len())];
        let attr = ATTRS[rng.index(ATTRS.len())];
        match rng.below(10) {
            0..6 => {
                let mut key = Self::attr_prefix(PREFIX_DATA, namespace, attr);
                key.extend_from_slice(&rng.between(1, 6).to_be_bytes());
                key
            }
            6..9 => {
                let mut key = Self::attr_prefix(PREFIX_INDEX, namespace, attr);
                key.extend_from_slice(TERMS[rng.index(TERMS.len())]);
                key
            }
            _ => self.schema_key(namespace, attr),
        }
    }

    pub fn schema_key(&self, namespace: u64, attr: &str) -> Vec<u8> {
        Self::attr_prefix(PREFIX_SCHEMA, namespace, attr)
    }

    /// A prefix to drop or scan: a namespace's keys of one kind, or one predicate's.
    pub fn prefix(&self, rng: &mut SimRng) -> Vec<u8> {
        let kinds = [PREFIX_DATA, PREFIX_INDEX, PREFIX_SCHEMA];
        let kind = kinds[rng.index(kinds.len())];
        let namespace = NAMESPACES[rng.index(NAMESPACES.len())];
        if rng.chance(500) {
            let mut prefix = vec![kind];
            prefix.extend_from_slice(&namespace.to_be_bytes());
            prefix
        } else {
            Self::attr_prefix(kind, namespace, ATTRS[rng.index(ATTRS.len())])
        }
    }

    /// A namespace other than the root one, which is never banned.
    pub fn bannable(&self, rng: &mut SimRng) -> u64 {
        NAMESPACES[1 + rng.index(NAMESPACES.len() - 1)]
    }
}
