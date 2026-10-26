/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::term_error::TermError;
use crate::types::encoded_term::VECTOR_MAX;

/// The elements of a vector literal's lexical form, which must be its canonical form: the elements in brackets,
/// separated by a comma and a space, each the shortest decimal that reads back as the same 32-bit float.
pub fn parse_vector(lexical: &str) -> Result<Vec<f32>, TermError> {
    let refuse = |reason| TermError::InvalidVector(lexical.to_string(), reason);
    let inner = lexical
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| refuse("not in brackets"))?;
    if inner.is_empty() {
        return Err(refuse("empty"));
    }
    let elements = inner
        .split(", ")
        .map(|e| {
            let v: f32 = e
                .parse()
                .map_err(|_| refuse("an element is no 32-bit float"))?;
            if !v.is_finite() {
                return Err(refuse("an element is not finite"));
            }
            if format_element(v) != e {
                return Err(refuse("not in canonical form"));
            }
            Ok(v)
        })
        .collect::<Result<Vec<f32>, TermError>>()?;
    if elements.len() > VECTOR_MAX {
        return Err(refuse("more than 65,535 elements"));
    }
    Ok(elements)
}

/// The canonical lexical form of `elements`.
pub fn format_vector(elements: impl Iterator<Item = f32>) -> String {
    let parts: Vec<String> = elements.map(format_element).collect();
    format!("[{}]", parts.join(", "))
}

/// The shortest decimal that reads back as `v`, with no exponent: Rust's `Display` for `f32`.
fn format_element(v: f32) -> String {
    format!("{v}")
}
