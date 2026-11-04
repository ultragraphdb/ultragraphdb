/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_role::RoleError;

/// Why one fetch of settings failed.
#[derive(Debug)]
pub(crate) enum FetchFailure {
    /// The service did not answer, which waiting may mend.
    Unreachable(String),
    /// The service refused, or its answer cannot run, which waiting does not mend.
    Refused(RoleError),
}
