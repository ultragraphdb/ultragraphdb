/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The unit tests of `ultragraphdb_store_apply`, mirroring `src/`: the changeset error, the applier, its executions,
//! writes, drops, builds, replays and vector indexing, over the simulated store.

#[cfg(test)]
mod errors;
#[cfg(test)]
mod types;
#[cfg(test)]
/// The helpers the unit tests share with the BDD target, which uses a part of them.
pub mod utils;
