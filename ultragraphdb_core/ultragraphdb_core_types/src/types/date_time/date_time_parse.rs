/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/time/format.go (parse, skip, getnum, parseNanoseconds, parseTimeZone, ParseError).
 * Changed for ultragraphdb: translated to Rust; the layout elements of the layouts Dgraph's ParseTime tries.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use crate::types::civil_time::CivilTime;
use crate::types::date_time::date_time_calendar::days_in;
use crate::types::date_time::{DateTime, Zone};

/// The layout elements Dgraph's layouts use, as Go's `nextStdChunk` classifies them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Std {
    LongYear,
    ZeroMonth,
    ZeroDay,
    Hour,
    ZeroMinute,
    ZeroSecond,
    Iso8601ColonTz,
    NumTz,
    Tz,
}

/// Go's layout `2006`.
pub(crate) const LAYOUT_Y: &str = "2006";
/// Go's layout `2006-01`.
pub(crate) const LAYOUT_YM: &str = "2006-01";
/// Go's layout `2006-01-02`.
pub(crate) const LAYOUT_YMD: &str = "2006-01-02";
/// Go's `time.RFC3339`.
pub(crate) const LAYOUT_RFC3339: &str = "2006-01-02T15:04:05Z07:00";
/// Dgraph's `dateFormatYMDZone`.
pub(crate) const LAYOUT_YMD_ZONE: &str = "2006-01-02 15:04:05 -0700 MST";
/// Dgraph's `dateTimeFormat`.
pub(crate) const LAYOUT_DATE_TIME: &str = "2006-01-02T15:04:05";

/// Go's `nextStdChunk` for the elements above: the literal prefix, the element and its text, and the rest.
fn next_std_chunk(layout: &str) -> (&str, Option<(Std, &str)>, &str) {
    const ELEMENTS: [(&str, Std); 9] = [
        ("2006", Std::LongYear),
        ("01", Std::ZeroMonth),
        ("02", Std::ZeroDay),
        ("15", Std::Hour),
        ("04", Std::ZeroMinute),
        ("05", Std::ZeroSecond),
        ("Z07:00", Std::Iso8601ColonTz),
        ("-0700", Std::NumTz),
        ("MST", Std::Tz),
    ];
    for i in 0..layout.len() {
        for (text, std) in ELEMENTS {
            if layout[i..].starts_with(text) {
                return (&layout[..i], Some((std, text)), &layout[i + text.len()..]);
            }
        }
    }
    (layout, None, "")
}

/// Go's `time.quote`: non-ASCII and control bytes as `\x` escapes, `"` and `\` backslashed.
fn quote(s: &[u8]) -> String {
    let mut out = String::from("\"");
    for &b in s {
        if !(b' '..0x80).contains(&b) {
            out.push_str(&format!("\\x{b:02x}"));
        } else {
            if b == b'"' || b == b'\\' {
                out.push('\\');
            }
            out.push(b as char);
        }
    }
    out.push('"');
    out
}

/// Go's `ParseError.Error`.
fn parse_error(
    layout: &str,
    value: &[u8],
    layout_elem: &str,
    value_elem: &[u8],
    message: &str,
) -> TypesError {
    if message.is_empty() {
        TypesError::time(format!(
            "parsing time {} as {}: cannot parse {} as {}",
            quote(value),
            quote(layout.as_bytes()),
            quote(value_elem),
            quote(layout_elem.as_bytes())
        ))
    } else {
        TypesError::time(format!("parsing time {}{message}", quote(value)))
    }
}

fn is_digit(s: &[u8], i: usize) -> bool {
    i < s.len() && s[i].is_ascii_digit()
}

/// Go's `getnum`: one or two digits (`fixed`: exactly two).
fn getnum(s: &[u8], fixed: bool) -> Option<(i64, &[u8])> {
    let b = s;
    if !is_digit(b, 0) {
        return None;
    }
    if !is_digit(b, 1) {
        if fixed {
            return None;
        }
        return Some((i64::from(b[0] - b'0'), &s[1..]));
    }
    Some((
        i64::from(b[0] - b'0') * 10 + i64::from(b[1] - b'0'),
        &s[2..],
    ))
}

/// Go's `atoi`: an optional sign and decimal digits, nothing else.
fn atoi(s: &[u8]) -> Option<i64> {
    let (neg, digits) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut x: u64 = 0;
    for &c in digits {
        if !c.is_ascii_digit() {
            return None;
        }
        if x > (1 << 63) / 10 {
            return None;
        }
        x = x * 10 + u64::from(c - b'0');
        if x > 1 << 63 {
            return None;
        }
    }
    // Go's int64(uint64): 1<<63, the one value past i64::MAX, reads as i64::MIN, which negates to itself.
    let x = x.cast_signed();
    Some(if neg { x.wrapping_neg() } else { x })
}

/// Go's `skip`: removes `prefix` from `value`, a space in the prefix matching any run of spaces.
fn skip<'a>(value: &'a [u8], prefix: &str) -> Result<&'a [u8], &'a [u8]> {
    let (mut value, mut prefix) = (value, prefix.as_bytes());
    while !prefix.is_empty() {
        if prefix[0] == b' ' {
            if !value.is_empty() && value[0] != b' ' {
                return Err(value);
            }
            prefix = cutspace(prefix);
            value = cutspace(value);
            continue;
        }
        if value.is_empty() || value[0] != prefix[0] {
            return Err(value);
        }
        prefix = &prefix[1..];
        value = &value[1..];
    }
    Ok(value)
}

fn cutspace(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|&&c| c == b' ').count();
    &s[n..]
}

/// Go's `parseNanoseconds`: a separator and digits, at most nine of them significant.
fn parse_nanoseconds(value: &[u8], nbytes: usize) -> Result<(i64, &'static str), ()> {
    let b = value;
    if b[0] != b'.' && b[0] != b',' {
        return Err(());
    }
    let nbytes = nbytes.min(10);
    let ns = atoi(&value[1..nbytes]).ok_or(())?;
    if ns < 0 {
        return Ok((0, "fractional second"));
    }
    Ok((ns * 10i64.pow((10 - nbytes) as u32), ""))
}

/// Go's `parseSignedOffset`: a signed hour offset in -23..=23 excluding 0; its length, or 0.
fn parse_signed_offset(value: &[u8]) -> usize {
    let b = value;
    if b.is_empty() || (b[0] != b'-' && b[0] != b'+') {
        return 0;
    }
    let digits = b[1..].iter().take_while(|c| c.is_ascii_digit()).count();
    let rem = &value[1 + digits..];
    let Some(x) = (digits > 0).then(|| atoi(&value[1..1 + digits])).flatten() else {
        return 0;
    };
    if x > 23 {
        return 0;
    }
    value.len() - rem.len()
}

/// Go's `parseTimeZone`: the length of a zone abbreviation at the start of `value`.
fn parse_time_zone(value: &[u8]) -> Option<usize> {
    let b = value;
    if b.len() < 3 {
        return None;
    }
    if b.len() >= 4 && (&value[..4] == b"ChST" || &value[..4] == b"MeST") {
        return Some(4);
    }
    if &value[..3] == b"GMT" {
        return Some(3 + parse_signed_offset(&value[3..]));
    }
    if b[0] == b'+' || b[0] == b'-' {
        let n = parse_signed_offset(value);
        return (n > 0).then_some(n);
    }
    let upper = b
        .iter()
        .take(6)
        .take_while(|c| c.is_ascii_uppercase())
        .count();
    match upper {
        5 if b[4] == b'T' => Some(5),
        4 if b[3] == b'T' || &value[..4] == b"WITA" => Some(4),
        3 => Some(3),
        _ => None,
    }
}

/// Go's `time.Parse(layout, value)` for Dgraph's layouts, with Go's error texts.
pub(crate) fn parse(layout_in: &str, value_in: &[u8]) -> Result<DateTime, TypesError> {
    let mut layout = layout_in;
    let mut value = value_in;
    let mut fields = Fields::new();
    loop {
        let (prefix, std, suffix) = next_std_chunk(layout);
        value = skip(value, prefix).map_err(|v| parse_error(layout_in, value_in, prefix, v, ""))?;
        let Some((std, stdstr)) = std else {
            if !value.is_empty() {
                return Err(parse_error(
                    layout_in,
                    value_in,
                    "",
                    value,
                    &format!(": extra text: {}", quote(value)),
                ));
            }
            break;
        };
        layout = suffix;
        let hold = value;
        let ok;
        (value, ok) = fields.parse_std(std, value);
        if !fields.range_err.is_empty() {
            return Err(parse_error(
                layout_in,
                value_in,
                stdstr,
                value,
                &format!(": {} out of range", fields.range_err),
            ));
        }
        if !ok {
            return Err(parse_error(layout_in, value_in, stdstr, hold, ""));
        }
    }
    fields.finish(layout_in, value_in, value)
}

/// The values Go's `parse` collects from a value's layout elements, and the first element out of range.
struct Fields<'a> {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    min: i64,
    sec: i64,
    nsec: i64,
    utc: bool,
    zone_offset: Option<i32>,
    zone_name: &'a [u8],
    range_err: &'static str,
}

impl<'a> Fields<'a> {
    fn new() -> Self {
        Fields {
            year: 0,
            month: -1,
            day: -1,
            hour: 0,
            min: 0,
            sec: 0,
            nsec: 0,
            utc: false,
            zone_offset: None,
            zone_name: b"",
            range_err: "",
        }
    }

    /// Parses one layout element from the start of `value`: the rest of the value, and whether the element parsed.
    fn parse_std(&mut self, std: Std, value: &'a [u8]) -> (&'a [u8], bool) {
        match std {
            Std::LongYear => {
                if value.len() < 4 || !is_digit(value, 0) {
                    return (value, false);
                }
                let (p, rest) = value.split_at(4);
                match atoi(p) {
                    Some(y) => {
                        self.year = y;
                        (rest, true)
                    }
                    None => (rest, false),
                }
            }
            Std::ZeroMonth => match getnum(value, true) {
                Some((m, rest)) => {
                    self.month = m;
                    if !(1..=12).contains(&m) {
                        self.range_err = "month";
                    }
                    (rest, true)
                }
                None => (value, false),
            },
            Std::ZeroDay => match getnum(value, true) {
                Some((d, rest)) => {
                    self.day = d;
                    (rest, true)
                }
                None => (value, false),
            },
            Std::Hour => match getnum(value, false) {
                Some((h, rest)) => {
                    self.hour = h;
                    if !(0..24).contains(&h) {
                        self.range_err = "hour";
                    }
                    (rest, true)
                }
                None => (value, false),
            },
            Std::ZeroMinute => match getnum(value, true) {
                Some((m, rest)) => {
                    self.min = m;
                    if !(0..60).contains(&m) {
                        self.range_err = "minute";
                    }
                    (rest, true)
                }
                None => (value, false),
            },
            Std::ZeroSecond => self.parse_second(value),
            Std::Iso8601ColonTz | Std::NumTz => self.parse_num_tz(std, value),
            Std::Tz => self.parse_tz(value),
        }
    }

    /// The `05` element, and a fractional second following it although the layout has none.
    fn parse_second(&mut self, input: &'a [u8]) -> (&'a [u8], bool) {
        let Some((s, mut value)) = getnum(input, true) else {
            return (input, false);
        };
        self.sec = s;
        if !(0..60).contains(&s) {
            self.range_err = "second";
            return (value, true);
        }
        let b = value;
        let mut ok = true;
        // A fractional second in the input although the layout has none.
        if b.len() >= 2 && (b[0] == b'.' || b[0] == b',') && is_digit(b, 1) {
            let n = 2 + b[2..].iter().take_while(|c| c.is_ascii_digit()).count();
            match parse_nanoseconds(value, n) {
                Ok((ns, rerr)) => {
                    self.nsec = ns;
                    self.range_err = rerr;
                }
                Err(()) => ok = false,
            }
            value = &value[n..];
        }
        (value, ok)
    }

    /// The `Z07:00` and `-0700` elements.
    fn parse_num_tz(&mut self, std: Std, input: &'a [u8]) -> (&'a [u8], bool) {
        if std == Std::Iso8601ColonTz && input.first() == Some(&b'Z') {
            self.utc = true;
            return (&input[1..], true);
        }
        let b = input;
        let (parts, value) = if std == Std::Iso8601ColonTz {
            if b.len() < 6 || b[3] != b':' {
                (None, input)
            } else {
                (
                    Some((&input[0..1], &input[1..3], &input[4..6])),
                    &input[6..],
                )
            }
        } else if b.len() < 5 {
            (None, input)
        } else {
            (
                Some((&input[0..1], &input[1..3], &input[3..5])),
                &input[5..],
            )
        };
        let Some((sign, hh, mm)) = parts else {
            return (value, false);
        };
        let (Some((hr, _)), Some((mi, _))) = (getnum(hh, true), getnum(mm, true)) else {
            return (value, false);
        };
        if hr > 24 {
            self.range_err = "time zone offset hour";
        }
        if mi > 60 {
            self.range_err = "time zone offset minute";
        }
        // Lossless: `getnum` reads two digits each, so the offset is at most 359,940 seconds.
        let off = ((hr * 60 + mi) * 60) as i32;
        match sign {
            b"+" => self.zone_offset = Some(off),
            b"-" => self.zone_offset = Some(-off),
            _ => return (value, false),
        }
        (value, true)
    }

    /// The `MST` element: `UTC` or a zone abbreviation.
    fn parse_tz(&mut self, value: &'a [u8]) -> (&'a [u8], bool) {
        if value.len() >= 3 && &value[..3] == b"UTC" {
            self.utc = true;
            return (&value[3..], true);
        }
        match parse_time_zone(value) {
            Some(n) => {
                self.zone_name = &value[..n];
                (&value[n..], true)
            }
            None => (value, false),
        }
    }

    /// The time the fields name, with defaults for a missing month and day and the zone the value gave.
    fn finish(
        self,
        layout_in: &str,
        value_in: &[u8],
        value: &[u8],
    ) -> Result<DateTime, TypesError> {
        let month = if self.month < 0 { 1 } else { self.month };
        let day = if self.day < 0 { 1 } else { self.day };
        if day < 1 || day > days_in(month, self.year) {
            return Err(parse_error(
                layout_in,
                value_in,
                "",
                value,
                ": day out of range",
            ));
        }
        let t = DateTime::new(
            CivilTime::new(
                self.year, month, day, self.hour, self.min, self.sec, self.nsec,
            ),
            Zone::Utc,
        );
        if self.utc {
            return Ok(t);
        }
        if let Some(offset) = self.zone_offset {
            return Ok(DateTime::from_unix_zone(
                t.unix() - i64::from(offset),
                t.nanosecond(),
                Zone::Fixed(offset),
            ));
        }
        if !self.zone_name.is_empty() {
            let mut offset = 0;
            if self.zone_name.len() > 3 && &self.zone_name[..3] == b"GMT" {
                // Lossless: `parse_signed_offset` admitted the hours only within 23 of zero.
                offset = atoi(&self.zone_name[3..]).unwrap_or(0) as i32 * 3600;
            }
            return Ok(t.with_zone(Zone::Fixed(offset)));
        }
        Ok(t)
    }
}
