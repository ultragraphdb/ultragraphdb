/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod migrate_view_body_view;

use crate::types::migrate_action::MigrateAction;
use crate::types::migrate_state::MigrateState;

/// The body of a received Migrate frame, read in place; see [`crate::Migrate`] for its layout. A body of `state` 0 is
/// a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrateView<'a> {
    tenant: u64,
    staged: u64,
    action: MigrateAction,
    state: Option<MigrateState>,
    refusal: u32,
    epoch: u64,
    message: &'a str,
    report: &'a str,
}

impl<'a> MigrateView<'a> {
    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The staged namespace's identifier.
    #[must_use]
    pub fn staged(&self) -> u64 {
        self.staged
    }

    /// The action asked.
    #[must_use]
    pub fn action(&self) -> MigrateAction {
        self.action
    }

    /// In a reply, the run's state; `None` in a request.
    #[must_use]
    pub fn state(&self) -> Option<MigrateState> {
        self.state
    }

    /// The UGS refusal number a failed run's check gave; 0 otherwise.
    #[must_use]
    pub fn refusal(&self) -> u32 {
        self.refusal
    }

    /// In a reply, the epoch the run entered its state at; 0 in a request.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// The refusal's message, borrowed from the frame; empty otherwise.
    #[must_use]
    pub fn message(&self) -> &'a str {
        self.message
    }

    /// The migration report, borrowed from the frame; empty in a request.
    #[must_use]
    pub fn report(&self) -> &'a str {
        self.report
    }
}
