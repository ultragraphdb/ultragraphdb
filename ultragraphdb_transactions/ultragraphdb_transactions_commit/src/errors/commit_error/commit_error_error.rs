/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::commit_error::{CommitError, CommitErrorEnum};

impl std::error::Error for CommitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            CommitErrorEnum::Malformed { cause } => Some(cause),
            _ => None,
        }
    }
}
