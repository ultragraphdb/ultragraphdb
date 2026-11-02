/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_log::LogError;

use crate::errors::cluster_error::ClusterError;

impl From<LogError> for ClusterError {
    fn from(error: LogError) -> Self {
        Self::log(error)
    }
}

impl From<InvariantError> for ClusterError {
    fn from(error: InvariantError) -> Self {
        Self::invariant(error)
    }
}
