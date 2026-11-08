/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::Reply;

use crate::errors::partition_error::PartitionError;

/// How a write a [`crate::PartitionHost`] took ended: its commit version or its refusal, with how many registrations it
/// waited for and how long, in the host's clock.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionOutcome {
    ticket: u64,
    outcome: Result<u64, PartitionError>,
    registrations: usize,
    waited_us: u64,
    reply: Option<Reply>,
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
            reply: None,
        }
    }

    /// The outcome carrying what the commit step answered.
    pub(crate) fn with_reply(mut self, reply: Reply) -> Self {
        self.reply = Some(reply);
        self
    }

    /// What the commit step answered: the commit's session token and its operations' results.
    #[must_use]
    pub fn reply(&self) -> Option<&Reply> {
        self.reply.as_ref()
    }

    /// The ticket [`crate::PartitionHost::submit`] gave the write.
    #[must_use]
    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    /// The commit version, or the refusal.
    #[must_use]
    pub fn outcome(&self) -> &Result<u64, PartitionError> {
        &self.outcome
    }

    /// How many registrations the write waited for.
    #[must_use]
    pub fn registrations(&self) -> usize {
        self.registrations
    }

    /// How long the write waited for its registrations' answers.
    #[must_use]
    pub fn waited_us(&self) -> u64 {
        self.waited_us
    }
}
