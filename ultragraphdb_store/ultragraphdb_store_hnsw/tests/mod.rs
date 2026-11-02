/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The tests of `ultragraphdb_store_hnsw`, mirroring `src/`: the HNSW index checked against golden vectors from
//! Dgraph's Go source, its decoders against corrupted values, and its workload on the simulated store and on fjall.

#[cfg(test)]
mod errors;
#[cfg(test)]
mod traits;
#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
