/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Compute's laws, L5.1 to L5.13 of `notes/laws.md`, as Hegel property tests mirroring `src/`.

// The played cluster of the unit tests, which the executor's laws read through (`AGENTS.md`: a harness included by path).
#[cfg(test)]
#[path = "../apart/mod.rs"]
pub mod apart;
#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
