/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The property tests of `ultragraphdb_protocol_wire`, mirroring `src/`: each states a law of a body over generated
//! inputs (`openspec/changes/stage-5-8-system-group/notes/laws.md`, group 1), its expected bytes and refusals laid out
//! by hand from the tables of `PROTOCOL.md` in `utils/layout.rs`.

#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
