/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Language-aware text: the collation order of a language and the base language of a tag, over ICU4X
//! (`icu_collator`, `icu_locale_core`) with its compiled CLDR data.
//!
//! - [`Collator::for_language`] is the collation of a BCP 47 language tag; [`Collator::compare`] orders two strings
//!   in it and [`Collator::key_bytes`] gives a string's sort key, whose bytes compare as the strings collate.
//! - [`Collator::compare_bytes`] and [`Collator::key_bytes`] take bytes that need not be valid UTF-8: each invalid
//!   sequence collates as U+FFFD.
//! - [`language_base`] is a tag's language subtag.
//!
//! ```
//! use std::cmp::Ordering;
//!
//! use ultragraphdb_core_text::{Collator, language_base};
//!
//! let de = Collator::for_language("de");
//! assert_eq!(
//!     de.as_ref().map(|de| de.compare("Äpfel", "Birnen")),
//!     Some(Ordering::Less)
//! );
//! assert_eq!(language_base("en-US").as_deref(), Some("en"));
//! ```

mod types;
mod utils;

pub use crate::types::collator::Collator;
pub use crate::utils::language_base::language_base;
