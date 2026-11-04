/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage role: partitions.

use std::process::ExitCode;

use mimalloc::MiMalloc;
use ultragraphdb_common_service::{RoleKind, exit_status, run_service};
use ultragraphdb_common_storage::storage_role;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A variable that is not Unicode names no setting, so it is left out here and never reaches the reading.
    let variables = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)));
    exit_status(run_service(
        RoleKind::Storage,
        &args,
        variables,
        storage_role,
    ))
}
