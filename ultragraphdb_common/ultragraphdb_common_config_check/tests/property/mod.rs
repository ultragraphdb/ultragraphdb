/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The property tests of `ultragraphdb_common_config_check`, mirroring `src/`: each states a law of the checks over drawn
//! targets (`openspec/changes/stage-5-8-system-group/notes/laws.md`, group 7), its expected verdict computed from what the
//! test drew by a rule written from the config-service specification, not by the checks' code.

#[cfg(test)]
mod utils;
