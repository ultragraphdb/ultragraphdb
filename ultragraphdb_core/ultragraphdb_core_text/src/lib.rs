/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Go's `golang.org/x/text` v0.41.0 (BSD-3-Clause, The Go Authors; see `LICENSE-GO-TEXT`): `collate`
//! with its CLDR 23 tables and `language`'s parsing and base inference, as Dgraph v25.5.0 uses them.
//!
//! - [`Tag::parse`] is `language.Parse`, [`Tag::base`] is `Tag.Base`, and [`language_base`] is the part of
//!   Dgraph's `tok.LangBase` that x/text answers; Dgraph's fallback to `en` is the caller's.
//! - [`Collator::new`] is `collate.New(tag)`, [`Collator::key`] is `KeyFromString` with Go's key format, and
//!   [`Collator::compare`] is `CompareString`.
//! - [`Collator::key_bytes`] and [`Collator::compare_bytes`] are `Key` and `Compare`, the same over bytes that need
//!   not be valid UTF-8, as a Go string's need not: Dgraph's string values are arbitrary bytes.
//!
//! ```
//! use ultragraphdb_core_text::{Collator, LanguageError, Tag};
//!
//! # fn main() -> Result<(), LanguageError> {
//! let de = Collator::new(&Tag::parse("de")?);
//! assert!(de.key("Äpfel") < de.key("Birnen"));
//! assert_eq!(de.compare("Äpfel", "Birnen"), std::cmp::Ordering::Less);
//! assert_eq!(de.compare_bytes(b"\xff", "\u{fffd}".as_bytes()), std::cmp::Ordering::Greater);
//! # Ok(())
//! # }
//! ```

mod errors;
mod tables;
mod types;
mod utils;

pub use crate::errors::language_error::{LanguageError, LanguageErrorEnum};
pub use crate::types::base::Base;
pub use crate::types::collator::Collator;
pub use crate::types::confidence::Confidence;
pub use crate::types::tag::Tag;
pub use crate::utils::language_base::language_base;
