/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The Go standard-library behaviour the port reproduces, once for every crate: one module per Go package,
//! named after it, and the types of `encoding/json`'s decoder, [`JsonDecoder`] and [`JsonValue`].
//!
//! ```
//! use ultragraphdb_common_utils::{JsonDecoder, go_json, go_strconv, go_strings};
//! use ultragraphdb_error_json::JsonError;
//!
//! # fn main() -> Result<(), JsonError> {
//! assert_eq!(go_strconv::parse_int("0x1f", 0), Ok(31));
//! assert_eq!(go_strconv::quote("a\u{7f}"), r#""a\x7f""#);
//! assert_eq!(go_strings::to_lower("İSTANBUL"), "istanbul");
//!
//! let mut dec = JsonDecoder::new(&br#"{"b": 1.50, "a": "\ud800"} x"#[..]);
//! assert_eq!(dec.decode()?.marshal(), "{\"a\":\"\u{fffd}\",\"b\":1.50}");
//! assert_eq!(
//!     dec.check_eof().unwrap_err().to_string(),
//!     "invalid character 'x' after top-level value"
//! );
//! assert_eq!(
//!     go_json::unmarshal(b"[1,]").unwrap_err().to_string(),
//!     "invalid character ']' looking for beginning of value"
//! );
//! # Ok(())
//! # }
//! ```

mod tables;
mod types;
mod utils;

pub use crate::types::json_decoder::JsonDecoder;
pub use crate::types::json_value::JsonValue;
pub use crate::utils::{go_fmt, go_json, go_strconv, go_strings, go_unicode, go_utf8};
