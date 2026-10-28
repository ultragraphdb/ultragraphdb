/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A commit request as it reaches the commit step: the caller's `ticket`, which the reply carries back to the
/// worker and connection that sent it, and the transaction's proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRequest<P> {
    ticket: u64,
    proposal: P,
}

impl<P> CommitRequest<P> {
    pub fn new(ticket: u64, proposal: P) -> Self {
        Self { ticket, proposal }
    }

    pub fn ticket(&self) -> u64 {
        self.ticket
    }

    pub fn proposal(&self) -> &P {
        &self.proposal
    }

    /// The ticket and the proposal.
    pub fn into_parts(self) -> (u64, P) {
        (self.ticket, self.proposal)
    }
}
