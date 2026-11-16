/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The property tests of `ultragraphdb_protocol_scram`, mirroring `src/`: each states a law of the exchange in two
//! halves over generated inputs (`openspec/changes/stage-5-8-system-group/notes/laws.md`, group 1b), with `ScramServer`
//! and `ScramClient` as the independent sources.

#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
