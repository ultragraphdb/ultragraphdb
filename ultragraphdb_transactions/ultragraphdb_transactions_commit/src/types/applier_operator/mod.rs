/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod applier_operator_operator;

use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::Batch;

/// The operator of a node's commit step: commit-time operations run on the changeset applier (7.5), each against the
/// store at the watermark with an overlay of the decided but unapplied proposals' writes above it, the round in flight's
/// and the current round's, in round order. The overlay is the posting lists' stacked layers ([`Batch::stack`]), built
/// when a round's first operation runs and dropped when the next round begins, so a round without operations stacks
/// nothing and allocates nothing for them.
#[derive(Debug)]
pub struct ApplierOperator<'a, S, C, E> {
    applier: &'a ChangesetApplier<S, C, E>,
    overlay: Option<Batch<'a, S, C>>,
}

impl<'a, S, C, E> ApplierOperator<'a, S, C, E> {
    pub fn new(applier: &'a ChangesetApplier<S, C, E>) -> Self {
        Self {
            applier,
            overlay: None,
        }
    }

    pub fn applier(&self) -> &'a ChangesetApplier<S, C, E> {
        self.applier
    }

    /// Whether the round's overlay is built: an operation ran in it.
    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }
}
