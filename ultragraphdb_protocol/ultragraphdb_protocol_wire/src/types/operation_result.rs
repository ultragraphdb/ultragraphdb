/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::constants::{ABSENT, OPERATION_RESULT_SIZE};

/// The result of a commit-time operation (`PROTOCOL.md` §6.3), 16 bytes: `value` i64, a counter add's new value; `node`
/// u32, the index in the reply's term table of the node an insert-if-absent or upsert-and-link found or created, named
/// there by its IRI or skolem IRI, [`crate::ABSENT`] for compare-and-set and counter add; `created` u8, whether the
/// operation created the node; then 3 zero bytes. A result naming a node has `value` 0, and one naming none `created`
/// false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationResult {
    value: i64,
    node: u32,
    created: bool,
}

impl OperationResult {
    /// The result of an insert-if-absent or upsert-and-link: the node at index `node` of the reply's term table, and
    /// whether the operation created it.
    #[must_use]
    pub fn for_node(node: u32, created: bool) -> Self {
        Self {
            value: 0,
            node,
            created,
        }
    }

    /// The result of a counter add whose counter now holds `value`.
    pub fn counter(value: i64) -> Self {
        Self {
            value,
            node: ABSENT,
            created: false,
        }
    }

    /// The result of a compare-and-set that held: no value, no node.
    #[must_use]
    pub fn compare_and_set() -> Self {
        Self::counter(0)
    }

    /// A counter add's new value; 0 for the other kinds.
    #[must_use]
    pub fn value(&self) -> i64 {
        self.value
    }

    /// The index in the reply's term table of the node found or created; `None` for compare-and-set and counter add.
    #[must_use]
    pub fn node(&self) -> Option<u32> {
        (self.node != ABSENT).then_some(self.node)
    }

    /// Whether the operation created the node; false for compare-and-set and counter add.
    pub fn created(&self) -> bool {
        self.created
    }

    /// The result's 16 bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; OPERATION_RESULT_SIZE] {
        let [v0, v1, v2, v3, v4, v5, v6, v7] = self.value.to_le_bytes();
        let [n0, n1, n2, n3] = self.node.to_le_bytes();
        let created = u8::from(self.created);
        [
            v0, v1, v2, v3, v4, v5, v6, v7, n0, n1, n2, n3, created, 0, 0, 0,
        ]
    }

    /// The result of `bytes`, which [`crate::utils::results::check_results`] checked: its fields as they lie.
    pub(crate) fn decode(bytes: &[u8; OPERATION_RESULT_SIZE]) -> Self {
        let [v0, v1, v2, v3, v4, v5, v6, v7, n0, n1, n2, n3, created, ..] = *bytes;
        Self {
            value: i64::from_le_bytes([v0, v1, v2, v3, v4, v5, v6, v7]),
            node: u32::from_le_bytes([n0, n1, n2, n3]),
            created: created == 1,
        }
    }
}
