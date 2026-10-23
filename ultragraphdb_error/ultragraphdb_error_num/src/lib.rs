/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `strconv.NumError`, the error every crate porting Go's number parsing raises. It names its input as text,
//! as Go's `strconv.Quote` writes it, so the crate depends on nothing and sits beneath the crates that raise it.

mod errors;

pub use crate::errors::num_error::{NumError, NumErrorEnum};
