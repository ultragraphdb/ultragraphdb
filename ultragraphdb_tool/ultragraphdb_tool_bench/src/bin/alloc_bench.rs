/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `alloc_bench`: the heap allocations between a DirectWrite's frame and its record (task 12.5b). See the crate's
//! documentation for the run.

use std::alloc::System;
use std::process::ExitCode;

use stats_alloc::{INSTRUMENTED_SYSTEM, StatsAlloc};
use ultragraphdb_tool_bench::{AllocArgs, AllocCount, run_allocs};

// The system allocator, every allocation counted: a count does not depend on which allocator serves it.
#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

/// The counts of the global allocator, read as the run reads them.
struct Counted;

impl AllocCount for Counted {
    fn counted(&self) -> (u64, u64, u64) {
        let stats = GLOBAL.stats();
        // A count of a 64-bit process fits a `u64`.
        let wide = |n: usize| n as u64;
        (
            wide(stats.allocations),
            wide(stats.reallocations),
            wide(stats.bytes_allocated),
        )
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match AllocArgs::parse(&args) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match run_allocs(&parsed, &Counted) {
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
