/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Go's `strconv.ParseUint(s, 0, 64)` for the numbers the schema lexer emits, a run of decimal digits or `0x` and
/// hexadecimal digits: base 16 after `0x` or `0X`, base 8 after a leading `0`, base 10 otherwise. The error is Go's
/// text, `strconv.ParseUint: parsing "<s>": invalid syntax` or `... value out of range`.
pub(crate) fn parse_uint_base0(s: &str) -> Result<u64, String> {
    let syntax = || format!("strconv.ParseUint: parsing \"{s}\": invalid syntax");
    let (digits, radix) = if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        (hex, 16)
    } else if s.len() > 1 && s.starts_with('0') {
        (&s[1..], 8)
    } else {
        (s, 10)
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return Err(syntax());
    }
    u64::from_str_radix(digits, radix)
        .map_err(|_| format!("strconv.ParseUint: parsing \"{s}\": value out of range"))
}
