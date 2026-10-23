/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The `Collator` trait: the language collation Dgraph's sort compares strings by, with the collation itself
//! supplied by the caller (`ultragraphdb_core_types::TextCollator` ports Go's `golang.org/x/text/collate`).

mod traits;

pub use crate::traits::collator::Collator;
