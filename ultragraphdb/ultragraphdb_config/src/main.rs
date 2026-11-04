/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The config service: every role's settings, per deployment target. `ultragraphdb_config --config <file>`.

use mimalloc::MiMalloc;
use std::process::ExitCode;
use ultragraphdb_common_config::run_config_service;
use ultragraphdb_common_service::exit_status;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    exit_status(run_config_service(&args))
}
