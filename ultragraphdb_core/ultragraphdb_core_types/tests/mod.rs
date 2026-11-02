/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The unit tests of `ultragraphdb_core_types`, mirroring `src/`: Dgraph's value types, their conversions, sorting
//! and geometry, checked against golden vectors from Dgraph's Go source.

#[cfg(test)]
mod errors;
#[cfg(test)]
mod s2;
#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
