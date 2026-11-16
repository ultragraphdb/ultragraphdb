/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::test_error::{TestError, TestResult};

/// Checks that `printed`, a value's `Debug` compact or pretty, holds no form of `secret`: its bytes as `Debug` prints
/// them, up to the closing bracket and whitespace aside, so a pretty print's one byte per line is found too; its
/// hexadecimal digits; and the secret itself when it is text.
///
/// # Errors
///
/// A [`TestError`] naming the form `printed` holds.
pub fn hides(printed: &str, secret: &[u8]) -> TestResult {
    let compact: String = printed.split_whitespace().collect();
    let bytes: String = format!("{secret:?}").split_whitespace().collect();
    let mut forms = vec![
        bytes.trim_end_matches(']').to_string(),
        secret.iter().map(|byte| format!("{byte:02x}")).collect(),
    ];
    forms.extend(std::str::from_utf8(secret).map(str::to_string));
    match forms
        .iter()
        .find(|form| printed.contains(form.as_str()) || compact.contains(form.as_str()))
    {
        Some(form) => Err(TestError::new(format!("{printed} holds {form}"))),
        None => Ok(()),
    }
}
