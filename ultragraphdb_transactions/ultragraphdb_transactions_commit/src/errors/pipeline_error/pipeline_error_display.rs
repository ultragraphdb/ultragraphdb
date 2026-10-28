/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::pipeline_error::{PipelineError, PipelineErrorEnum};

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            PipelineErrorEnum::Log { cause } => {
                write!(f, "the log refused or failed a round: {cause}")
            }
            PipelineErrorEnum::Apply { cause } => {
                write!(f, "the store refused a synced round: {cause}")
            }
            PipelineErrorEnum::Sessions { cause } => {
                write!(f, "the store's session table could not be read: {cause}")
            }
            PipelineErrorEnum::BuildEnd { cause } => {
                write!(f, "a build's end could not be proposed: {cause}")
            }
        }
    }
}
