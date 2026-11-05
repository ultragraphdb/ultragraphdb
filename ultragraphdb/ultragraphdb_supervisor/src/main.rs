/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The supervisor of the development container: the config service and the three roles, started in order, stopped in order,
//! and the first failure's code reported. `ultragraphdb_supervisor --config <file>`.

mod service;

use std::process::ExitCode;

use mimalloc::MiMalloc;
use ultragraphdb_common_service::exit_status;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match service::run_supervisor(&args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => exit_status(Err(error)),
    }
}
