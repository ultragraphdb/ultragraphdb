/*
 * Copyright 2022 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/time/format_rfc3339.go and format.go (appendInt, appendNano). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::date_time::DateTime;

/// Go's `appendInt`: `x` in decimal, zero-padded to `width` digits, with a leading `-` when negative.
fn append_int(b: &mut String, x: i64, width: usize) {
    if x < 0 {
        b.push('-');
    }
    let digits = x.unsigned_abs().to_string();
    (digits.len()..width).for_each(|_| b.push('0'));
    b.push_str(&digits);
}

impl DateTime {
    /// Go's `MarshalText`: RFC 3339 with nanoseconds, trailing zeros trimmed, `Z` for a zero offset.
    pub fn marshal_text(&self) -> Result<String, TypesError> {
        let c = self.civil();
        let mut b = String::with_capacity(35);
        append_int(&mut b, c.year, 4);
        b.push('-');
        append_int(&mut b, c.month, 2);
        b.push('-');
        append_int(&mut b, c.day, 2);
        b.push('T');
        append_int(&mut b, c.hour, 2);
        b.push(':');
        append_int(&mut b, c.min, 2);
        b.push(':');
        append_int(&mut b, c.sec, 2);
        let ns = self.nanosecond();
        if ns != 0 {
            let frac = format!("{ns:09}");
            b.push('.');
            b.push_str(frac.trim_end_matches('0'));
        }
        if !(0..=9999).contains(&c.year) {
            return Err(TypesError::time(
                "Time.MarshalText: year outside of range [0,9999]",
            ));
        }
        let offset = self.offset();
        if offset == 0 {
            b.push('Z');
            return Ok(b);
        }
        let mut zone = offset / 60;
        if zone < 0 {
            b.push('-');
            zone = -zone;
        } else {
            b.push('+');
        }
        if zone / 60 >= 24 {
            return Err(TypesError::time(
                "Time.MarshalText: timezone hour outside of range [0,23]",
            ));
        }
        append_int(&mut b, i64::from(zone / 60), 2);
        b.push(':');
        append_int(&mut b, i64::from(zone % 60), 2);
        Ok(b)
    }
}
