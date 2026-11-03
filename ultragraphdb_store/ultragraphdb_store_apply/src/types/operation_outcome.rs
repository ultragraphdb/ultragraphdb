/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// What one commit-time operation gave: the node an insert-if-absent or an upsert-and-link found or created, a
/// compare-and-set that held its expected value, or a counter's new value. The node is the store's system ID, which
/// the protocol names to a client by its IRI or skolem IRI, never by the ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    /// The node found or created, and whether the operation created it.
    Node {
        /// The node.
        node: SystemId,
        /// Whether the operation created it.
        created: bool,
    },
    /// A compare-and-set whose expected value was held, now replaced.
    Set,
    /// A counter add: the counter's new value.
    Counter(i64),
}
