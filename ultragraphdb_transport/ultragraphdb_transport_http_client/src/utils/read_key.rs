/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::http_client_error::{HttpClientError, HttpClientErrorEnum};

/// The key `file` holds: its contents without a final line break.
///
/// # Errors
///
/// [`HttpClientErrorEnum::Key`] naming the file, for one that cannot be read as text, holds no key, or holds a key
/// a header cannot carry.
pub fn read_key(file: &str) -> Result<String, HttpClientError> {
    let refused = |reason: String| {
        HttpClientError(HttpClientErrorEnum::Key {
            file: file.to_string(),
            reason,
        })
    };
    let contents = std::fs::read_to_string(file).map_err(|error| refused(error.to_string()))?;
    let line = contents.strip_suffix('\n').unwrap_or(&contents);
    let key = line.strip_suffix('\r').unwrap_or(line);
    if key.is_empty() {
        return Err(refused("it is empty".to_string()));
    }
    if key.chars().any(char::is_control) {
        return Err(refused("it holds a control character".to_string()));
    }
    Ok(key.to_string())
}
