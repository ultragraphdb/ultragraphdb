/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply, Join, Members, RoleKind};

/// The roles that joined (`PROTOCOL.md` §11.12, §13.1): each partition's holder over the placement's partitions and
/// each compute process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Membership {
    partition_count: u32,
    partitions: BTreeMap<u32, String>,
    compute: BTreeSet<String>,
}

impl Membership {
    /// No role yet, over partitions 1 to `partition_count`.
    #[must_use]
    pub fn new(partition_count: u32) -> Self {
        Self {
            partition_count,
            ..Self::default()
        }
    }

    /// The Join `join` registered, replacing what its address held.
    ///
    /// # Errors
    ///
    /// An [`ErrorReply`] of code 1 naming the partition outside the placement, or naming it and its holder when another
    /// address holds it; nothing of the Join is registered.
    pub fn join(&mut self, join: &Join) -> Result<(), ErrorReply> {
        let address = join.address();
        if join.kind() == RoleKind::Compute {
            let _new = self.compute.insert(address.to_string());
            return Ok(());
        }
        let claimed = join.partitions();
        for partition in claimed {
            if *partition > self.partition_count {
                return Err(protocol(format!(
                    "partition {partition} is outside the placement of {}",
                    self.partition_count
                )));
            }
            if let Some(holder) = self
                .partitions
                .get(partition)
                .filter(|holder| *holder != address)
            {
                return Err(protocol(format!(
                    "partition {partition} is held by {holder}"
                )));
            }
        }
        self.partitions.retain(|_, holder| holder != address);
        self.partitions.extend(
            claimed
                .iter()
                .map(|partition| (*partition, address.to_string())),
        );
        Ok(())
    }

    /// The members in the order `PROTOCOL.md` §13.2 sets: compute by address, then storage by partition.
    #[must_use]
    pub fn members(&self) -> Members {
        let mut members = Members::new();
        for address in &self.compute {
            members.push_compute(address.as_str());
        }
        for (partition, address) in &self.partitions {
            members.push_storage(*partition, address.as_str());
        }
        members
    }
}

fn protocol(message: String) -> ErrorReply {
    ErrorReply::new(ErrorCode::Protocol, message)
}
