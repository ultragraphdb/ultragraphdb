/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Go standard-library behaviour the port reproduces, once for every crate: one module per Go package,
//! named after it.
//!
//! ```
//! use ultragraphdb_common_utils::{go_strconv, go_strings};
//!
//! assert_eq!(go_strconv::parse_int("0x1f", 0), Ok(31));
//! assert_eq!(go_strconv::quote("a\u{7f}"), r#""a\x7f""#);
//! assert_eq!(go_strings::to_lower("İSTANBUL"), "istanbul");
//! ```

mod tables;
mod utils;

pub use crate::utils::{go_fmt, go_strconv, go_strings, go_unicode, go_utf8};
