/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `store_bench`: stage 2's storage measurements (task 10.2). See the crate's documentation for the runs.

use std::process::ExitCode;

use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_tool_bench::{Command, run_apply, run_rollup};

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
    let report = match Command::parse(&args) {
        Ok(Command::Apply(a)) => run_apply(&a),
        Ok(Command::Rollup(r)) => run_rollup(&r),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match report {
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
