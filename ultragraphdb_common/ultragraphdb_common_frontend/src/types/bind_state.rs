/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::role_link::RoleLink;

/// Where a read-write transaction stands with its partition.
#[derive(Debug)]
pub(crate) enum BindState {
    /// Nothing has bound it.
    None,
    /// Its Bind is sent and unanswered.
    Binding {
        link: RoleLink,
        stream: u64,
        partition: u32,
    },
    /// Its partition answered, naming the version it reads at.
    Bound {
        link: RoleLink,
        stream: u64,
        partition: u32,
        read_version: u64,
    },
}
