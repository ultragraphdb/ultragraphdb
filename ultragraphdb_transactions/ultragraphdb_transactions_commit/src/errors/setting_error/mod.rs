/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod setting_error_display;
mod setting_error_error;

/// Why a setting of the commit step was refused at start: the role stops with exit code 78 naming it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingError(pub SettingErrorEnum);

/// The reasons, each naming the setting and the value refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingErrorEnum {
    /// `close_request_ms` is `value`, outside 1 to 60,000.
    CloseRequestRange {
        /// The value refused, in milliseconds.
        value: u64,
    },
    /// `close_request_ms` is `value`, below twice `epoch_ms_max`, so a reachable sequencer could leave a close
    /// request unanswered for longer and the partition refuse its commits as busy.
    CloseRequestBelowEpochs {
        /// The value refused, in milliseconds.
        value: u64,
        /// The sequencer's `epoch_ms_max`, in milliseconds.
        epoch_ms_max: u64,
    },
}

impl SettingError {
    /// The [`SettingErrorEnum::CloseRequestRange`] reason.
    #[must_use]
    pub fn close_request_range(value: u64) -> Self {
        Self(SettingErrorEnum::CloseRequestRange { value })
    }

    /// The [`SettingErrorEnum::CloseRequestBelowEpochs`] reason.
    #[must_use]
    pub fn close_request_below_epochs(value: u64, epoch_ms_max: u64) -> Self {
        Self(SettingErrorEnum::CloseRequestBelowEpochs {
            value,
            epoch_ms_max,
        })
    }
}
