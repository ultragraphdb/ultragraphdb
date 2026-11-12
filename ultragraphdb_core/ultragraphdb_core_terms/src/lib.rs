/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! RDF terms as the store keeps them: the term model on `oxrdf`, `EncodedTerm` with its encoding by content, the
//! canonical-form test, the 128-bit hash naming a dictionary entry, and the posting fingerprint.

// Arithmetic on decoded store/value text input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

mod errors;
mod types;
mod utils;

pub use crate::errors::term_error::{TermError, TermErrorEnum};
pub use crate::types::bfloat16_vector_view::Bfloat16VectorView;
pub use crate::types::datatype::Datatype;
pub use crate::types::encoded_literal::EncodedLiteral;
pub use crate::types::encoded_term::{ENCODED_MAX, EncodedTerm, INLINE_MAX, VECTOR_MAX};
pub use crate::types::hash128::Hash128;
pub use crate::types::known_datatype::{FLOAT32_VECTOR, KnownDatatype, SYSTEM_VOCABULARY};
pub use crate::types::tagged_datatype::TaggedDatatype;
pub use crate::types::term_view::TermView;
pub use crate::types::text::Text;
pub use crate::types::vector_view::VectorView;
pub use crate::utils::vector::{format_vector, parse_vector};
