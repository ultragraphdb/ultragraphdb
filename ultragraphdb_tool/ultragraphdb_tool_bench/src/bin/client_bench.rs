/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `client_bench`: requests through the three binaries with the real client (task 12.5b). See the crate's documentation
//! for the run.

use std::process::ExitCode;

use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_tool_bench::{ClientArgs, run_client};

// musl's allocator contends under threads; mimalloc is every shipped binary's allocator (the workspace's `mimalloc` pin).
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> ExitCode {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".to_string());
    let _telemetry = match init(&TelemetryConfig::new(filter, LogFormat::Plain)) {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match ClientArgs::parse(&args) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match run_client(&parsed) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
