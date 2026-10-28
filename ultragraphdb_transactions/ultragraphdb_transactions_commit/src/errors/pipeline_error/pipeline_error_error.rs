/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::pipeline_error::{PipelineError, PipelineErrorEnum};

impl std::error::Error for PipelineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            PipelineErrorEnum::Log { cause } => Some(cause),
            PipelineErrorEnum::Apply { cause } => Some(cause),
            PipelineErrorEnum::Sessions { cause } => Some(cause),
        }
    }
}
