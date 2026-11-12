/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Loom tests, under `tests/loom/`, mirroring `src/`: each runs its model under every interleaving of its threads
//! and every ordering their atomics allow, against the crate built with `--cfg loom`.

#[cfg(test)]
mod types;
