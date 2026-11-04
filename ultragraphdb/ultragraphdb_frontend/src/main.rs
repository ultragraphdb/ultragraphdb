/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend role: the client endpoint, authentication, the metadata group and the sequencer.

use std::process::ExitCode;

use mimalloc::MiMalloc;
use ultragraphdb_common_service::{RoleKind, exit_status, run_role};

// musl's allocator contends under threads; mimalloc is every shipped binary's allocator.
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A variable that is not Unicode names no setting, so it is left out here and never reaches the reading.
    let variables = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)));
    exit_status(run_role(RoleKind::Frontend, &args, variables))
}
