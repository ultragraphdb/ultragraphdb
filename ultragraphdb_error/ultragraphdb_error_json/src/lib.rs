/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's `encoding/json` errors, raised by the port of its decoder in `ultragraphdb_common_utils` and carried by the
//! crates that decode JSON with it. They name their input as text, so the crate depends on nothing and sits beneath
//! the crates that raise them.

mod errors;

pub use crate::errors::json_error::{JsonError, JsonErrorEnum};
