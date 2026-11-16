/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The property tests of `ultragraphdb_cluster_partition`, mirroring `src/`: each states a law of the partition over drawn
//! inputs (`openspec/changes/stage-5-8-system-group/notes/laws.md`, group 4), its expected values computed from what was
//! drawn by a model written from the specification.

#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
