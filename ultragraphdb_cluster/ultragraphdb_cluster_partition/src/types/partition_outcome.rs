/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::partition_error::PartitionError;

/// How a write a [`crate::PartitionHost`] took ended: its commit version or its refusal, with how many registrations it
/// waited for and how long, in the host's clock.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionOutcome {
    ticket: u64,
    outcome: Result<u64, PartitionError>,
    registrations: usize,
    waited_us: u64,
}

impl PartitionOutcome {
    pub(crate) fn new(
        ticket: u64,
        outcome: Result<u64, PartitionError>,
        registrations: usize,
        waited_us: u64,
    ) -> Self {
        Self {
            ticket,
            outcome,
            registrations,
            waited_us,
        }
    }

    /// The ticket [`crate::PartitionHost::submit`] gave the write.
    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    /// The commit version, or the refusal.
    pub fn outcome(&self) -> &Result<u64, PartitionError> {
        &self.outcome
    }

    /// How many registrations the write waited for.
    pub fn registrations(&self) -> usize {
        self.registrations
    }

    /// How long the write waited for its registrations' answers.
    pub fn waited_us(&self) -> u64 {
        self.waited_us
    }
}
