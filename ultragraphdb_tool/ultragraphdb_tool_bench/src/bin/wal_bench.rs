/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `wal_bench`: stage 5's measurement of the write-ahead log beside fjall's journal (task 12.5). See the crate's
//! documentation for the run.

use std::process::ExitCode;

use ultragraphdb_common_telemetry::{LogFormat, TelemetryConfig, init};
use ultragraphdb_tool_bench::{PartitionArgs, PipelineArgs, run_partitions, run_pipeline};

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
    // `--partitions` selects the run over partitions; without it, the run of one log beside fjall's journal.
    let report = if args.iter().any(|a| a == "--partitions") {
        PartitionArgs::parse(&args).map(|a| run_partitions(&a))
    } else {
        PipelineArgs::parse(&args).map(|a| run_pipeline(&a))
    };
    let report = match report {
        Ok(ran) => ran,
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
