/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The unit tests of `ultragraphdb_core_tok`, mirroring `src/`: Dgraph's tokenizers and bleve's analysis, checked
//! against golden vectors from Dgraph's Go source.

#[cfg(test)]
mod bleve;
#[cfg(test)]
mod errors;
#[cfg(test)]
mod golden;
#[cfg(test)]
mod tables;
#[cfg(test)]
mod types;
#[cfg(test)]
mod utils;
