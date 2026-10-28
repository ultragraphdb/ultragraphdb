/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::OperationResult;

use crate::errors::commit_error::CommitError;

/// What runs a proposal's commit-time operations for the commit step (7.5): after the proposal is admitted by the
/// resolver and before it takes a version, against the store at the watermark with the writes of every decided but
/// unapplied proposal stacked above it, the round in flight's and the current round's, in round order. The step tells
/// it when a round begins and each proposal it admits, so it keeps that overlay itself; a node's operator holds the
/// changeset applier ([`crate::ApplierOperator`]), a step without one refuses every operation ([`crate::NoOperations`]).
pub trait Operator<P> {
    /// A proposal as it commits once its operations have run, not yet taken as the proposal's.
    type Staged;

    /// A round begins: the proposals of the round before the one in flight are applied by now.
    fn begin_round(&mut self);

    /// Runs `proposal`'s operations over `unapplied`, every decided but unapplied proposal in round order, appending
    /// their results to `results`. `proposal` is left as it was.
    ///
    /// # Errors
    ///
    /// [`CommitError::OperationFailed`] for a compare-and-set that did not hold; [`CommitError::Refused`] for an
    /// operation the catalog or the store refused.
    fn operate<'p, I>(
        &mut self,
        proposal: &P,
        unapplied: I,
        results: &mut Vec<OperationResult>,
    ) -> Result<Self::Staged, CommitError>
    where
        I: Iterator<Item = &'p P>,
        P: 'p;

    /// The record `staged` logs.
    fn staged_record<'s>(&self, staged: &'s Self::Staged) -> &'s [u8];

    /// Takes `staged` as `proposal`'s writes and record.
    fn adopt(&mut self, proposal: &mut P, staged: Self::Staged);

    /// `proposal` was admitted: its writes are decided and unapplied until its round is.
    fn admitted(&mut self, proposal: &P);
}
