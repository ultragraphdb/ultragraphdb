/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The tests of `ultragraphdb_tool_bench`, mirroring `src/`: the argument parsing, the commit shape, the pacer
//! against a clock moved by hand, the summaries, the timed store, the directory sizes, and short runs of each
//! measurement.

#[cfg(test)]
mod errors;
#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
