/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/encoding/json/encode.go (floatEncoder). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_strconv;

/// `encoding/json`'s layout of a finite float64: `f` format, or `e` below 1e-6 and from 1e21, with `e-07`
/// shortened to `e-7`. `None` for NaN and infinities, which `json.Marshal` refuses.
pub(crate) fn json_float(f: f64) -> Option<String> {
    if !f.is_finite() {
        return None;
    }
    let abs = f.abs();
    let fmt = if abs != 0.0 && (abs < 1e-6 || abs >= 1e21) {
        b'e'
    } else {
        b'f'
    };
    let mut s = go_strconv::format_float(f, fmt, 64);
    if fmt == b'e' {
        let b = s.as_bytes();
        let n = b.len();
        if n >= 4 && b[n - 4] == b'e' && b[n - 3] == b'-' && b[n - 2] == b'0' {
            let last = b[n - 1] as char;
            s.truncate(n - 2);
            s.push(last);
        }
    }
    Some(s)
}
