/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `StoreError`, why a store operation failed: raised by both implementations of `ultragraphdb_traits_store`'s
//! `Store`, the one over fjall and the simulated one, and carried by every storage crate above them.

mod errors;
mod utils;

pub use crate::errors::store_error::{StoreError, StoreErrorEnum};
