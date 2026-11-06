/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the crate's public surface does with no process: a merge's frontier and the keys a read scans by. Reads themselves run
//! against real storage processes in `ultragraphdb_compute_tests`.

mod types;
mod utils;
