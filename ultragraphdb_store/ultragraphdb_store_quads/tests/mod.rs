/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The tests of `ultragraphdb_store_quads`, mirroring `src/`: the term dictionary, quad writes and reads, and the data
//! dictionary over a fault-free simulated store, and the dictionary workload on the simulator.

#[cfg(test)]
mod errors;
#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
