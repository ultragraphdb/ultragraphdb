/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/fmt/print.go (%v of a slice) and src/fmt/format.go (fmtUnicode). Changed for
 * ultragraphdb: translated to Rust; the verbs Dgraph's messages print.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `fmt` verbs as Dgraph's messages use them: `%v` of a `[]byte` and of a `[]string`, and `%#U` of a rune.
//! `%q` is [`go_strconv::quote`](crate::go_strconv::quote).

use crate::utils::isprint::is_print;

/// `%v` of a `[]byte`: `[1 2 3]`.
pub fn byte_slice(bytes: &[u8]) -> String {
    let items: Vec<String> = bytes.iter().map(u8::to_string).collect();
    format!("[{}]", items.join(" "))
}

/// `%v` of a `[]string`: `[a b c]`.
pub fn string_slice(v: &[String]) -> String {
    format!("[{}]", v.join(" "))
}

/// `%#U` of a rune, `U+0041 'A'`, the rune itself only when Go's `strconv.IsPrint` accepts it; the end of the input
/// is Go's rune -1, `U+FFFFFFFFFFFFFFFF`.
pub fn unicode_hash(r: Option<char>) -> String {
    match r {
        None => "U+FFFFFFFFFFFFFFFF".to_string(),
        Some(c) if is_print(c) => format!("U+{:04X} '{c}'", c as u32),
        Some(c) => format!("U+{:04X}", c as u32),
    }
}
