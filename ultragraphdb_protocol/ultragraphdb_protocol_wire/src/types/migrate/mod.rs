/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod migrate_body_builder;

use crate::types::migrate_action::MigrateAction;
use crate::types::migrate_state::MigrateState;

/// One action of a migration's run, named by the `staged` identifier its Install answered (`PROTOCOL.md` §8.6): the
/// frontend answers once the metadata group has committed the transition asked for, a state read at once. Refused with
/// code 7 before stage 9.5.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `tenant` | u64 |
/// | 8 | `staged` | u64: the staged namespace's identifier, at least 1 |
/// | 16 | `action` | u8: 1 start, 2 dry run, 3 cut over, 4 cancel, 5 read the state |
/// | 17 | `state` | u8: in a reply, the run's state, 1 to 9; 0 in a request |
/// | 20 | `refusal` | u32: in a reply of state 9 for a run a check failed, its UGS refusal number, 2201 to 2299; else 0 |
/// | 24 | `epoch` | epoch: in a reply, the epoch the run entered its state at; 0 in a request |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the message, UTF-8: in a reply of state 9 for a run a check failed, the refusal's message; else empty |
/// | 1 | the migration report, UTF-8, as the run has brought it; empty in a request |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migrate {
    tenant: u64,
    staged: u64,
    action: MigrateAction,
    state: Option<MigrateState>,
    refusal: u32,
    epoch: u64,
    message: String,
    report: String,
}

impl Migrate {
    /// A request asking `action` of the run of the staged namespace `staged` in `tenant`.
    #[must_use]
    pub fn new(tenant: u64, staged: u64, action: MigrateAction) -> Self {
        Self {
            tenant,
            staged,
            action,
            state: None,
            refusal: 0,
            epoch: 0,
            message: String::new(),
            report: String::new(),
        }
    }

    /// The reply: the run's `state`, the epoch it entered it at, and its migration `report` so far.
    #[must_use]
    pub fn answered(self, state: MigrateState, epoch: u64, report: impl Into<String>) -> Self {
        Self {
            state: Some(state),
            epoch,
            report: report.into(),
            ..self
        }
    }

    /// The reply of a run a check failed: the UGS `refusal` number it failed with and the refusal's `message`.
    #[must_use]
    pub fn refused(self, refusal: u32, message: impl Into<String>) -> Self {
        Self {
            refusal,
            message: message.into(),
            ..self
        }
    }

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

    /// The refusal's message; empty otherwise.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The migration report as the run has brought it; empty in a request.
    #[must_use]
    pub fn report(&self) -> &str {
        &self.report
    }
}
