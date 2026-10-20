/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26, src/encoding/json/scanner.go (syntax errors) and decode.go (type errors). Changed for
 * ultragraphdb: translated to Rust; a JSON parser that keeps numbers as their text, as GeoJSON decoding needs it.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The subset of Go's `encoding/json` that go-geom's GeoJSON decoding goes through: syntax errors with Go's
//! texts, values with numbers kept as text, and Go's `UnmarshalTypeError` texts.

/// A parsed JSON value. Object members keep their order and duplicates, as Go's decoder sees them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// The kind Go names in `cannot unmarshal <kind> into ...`.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "bool",
            JsonValue::Number(_) => "number",
            JsonValue::String(_) => "string",
            JsonValue::Array(_) => "array",
            JsonValue::Object(_) => "object",
        }
    }
}

/// Go's `UnmarshalTypeError` text for a value decoded into a Go type outside a struct.
pub(crate) fn type_error(value: &str, go_type: &str) -> String {
    format!("json: cannot unmarshal {value} into Go value of type {go_type}")
}

/// Go's `quoteChar`.
fn quote_char(c: u8) -> String {
    match c {
        b'\'' => "'\\''".into(),
        b'"' => "'\"'".into(),
        _ => {
            let s = crate::utils::strconv::quote(char::from(c).to_string().as_bytes());
            format!("'{}'", &s[1..s.len() - 1])
        }
    }
}

fn invalid(c: u8, context: &str) -> String {
    format!("invalid character {} {context}", quote_char(c))
}

const UNEXPECTED_END: &str = "unexpected end of JSON input";

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

impl Parser<'_> {
    /// Go's syntax error at the current position, naming the character there (a whole UTF-8 character).
    fn invalid(&self, context: &str) -> String {
        let rest = &self.s[self.pos..];
        let n = rest.len().min(4);
        let ch = (1..=n).find_map(|k| {
            std::str::from_utf8(&rest[..k])
                .ok()
                .and_then(|t| t.chars().next())
        });
        match ch {
            Some(c) if !c.is_ascii() => {
                let q = crate::utils::strconv::quote(c.to_string().as_bytes());
                format!("invalid character '{}' {context}", &q[1..q.len() - 1])
            }
            Some(_) => invalid(rest[0], context),
            None => format!("invalid character '\\x{:02x}' {context}", rest[0]),
        }
    }

    fn skip_space(&mut self) {
        while self.pos < self.s.len() && is_space(self.s[self.pos]) {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn value(&mut self) -> Result<JsonValue, String> {
        self.skip_space();
        let c = self.peek().ok_or(UNEXPECTED_END)?;
        match c {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(JsonValue::String),
            b'-' | b'0'..=b'9' => self.number().map(JsonValue::Number),
            b't' => self.literal(b"true", JsonValue::Bool(true)),
            b'f' => self.literal(b"false", JsonValue::Bool(false)),
            b'n' => self.literal(b"null", JsonValue::Null),
            _ => Err(self.invalid("looking for beginning of value")),
        }
    }

    fn literal(&mut self, word: &[u8], v: JsonValue) -> Result<JsonValue, String> {
        let name = std::str::from_utf8(word).expect("ASCII");
        for (i, &w) in word.iter().enumerate() {
            let c = self.peek().ok_or(UNEXPECTED_END)?;
            if c != w {
                return Err(self.invalid(&format!(
                    "in literal {name} (expecting {})",
                    quote_char(word[i])
                )));
            }
            self.pos += 1;
        }
        Ok(v)
    }

    fn object(&mut self) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut members = Vec::new();
        self.skip_space();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(members));
        }
        loop {
            self.skip_space();
            let c = self.peek().ok_or(UNEXPECTED_END)?;
            if c != b'"' {
                return Err(self.invalid("looking for beginning of object key string"));
            }
            let key = self.string()?;
            self.skip_space();
            let c = self.peek().ok_or(UNEXPECTED_END)?;
            if c != b':' {
                return Err(self.invalid("after object key"));
            }
            self.pos += 1;
            let v = self.value()?;
            members.push((key, v));
            self.skip_space();
            match self.peek().ok_or(UNEXPECTED_END)? {
                b',' => self.pos += 1,
                b'}' => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(members));
                }
                _ => return Err(self.invalid("after object key:value pair")),
            }
        }
    }

    fn array(&mut self) -> Result<JsonValue, String> {
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_space();
            match self.peek().ok_or(UNEXPECTED_END)? {
                b',' => self.pos += 1,
                b']' => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                _ => return Err(self.invalid("after array element")),
            }
        }
    }

    fn number(&mut self) -> Result<String, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek().ok_or(UNEXPECTED_END)? {
            b'0' => self.pos += 1,
            b'1'..=b'9' => {
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.invalid("in numeric literal")),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            let c = self.peek().ok_or(UNEXPECTED_END)?;
            if !c.is_ascii_digit() {
                return Err(self.invalid("after decimal point in numeric literal"));
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let c = self.peek().ok_or(UNEXPECTED_END)?;
            if !c.is_ascii_digit() {
                return Err(self.invalid("in exponent of numeric literal"));
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        Ok(String::from_utf8(self.s[start..self.pos].to_vec()).expect("ASCII number"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut v = 0;
        for _ in 0..4 {
            let c = self.peek().ok_or(UNEXPECTED_END)?;
            let d = (c as char)
                .to_digit(16)
                .ok_or_else(|| self.invalid("in \\u hexadecimal character escape"))?;
            v = v * 16 + d;
            self.pos += 1;
        }
        Ok(v)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let c = self.peek().ok_or(UNEXPECTED_END)?;
            self.pos += 1;
            match c {
                b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let e = self.peek().ok_or(UNEXPECTED_END)?;
                    self.pos += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut r = self.hex4()?;
                            if (0xd800..0xdc00).contains(&r)
                                && self.s[self.pos..].starts_with(b"\\u")
                            {
                                let save = self.pos;
                                self.pos += 2;
                                let r2 = self.hex4()?;
                                if (0xdc00..0xe000).contains(&r2) {
                                    r = 0x10000 + ((r - 0xd800) << 10) + (r2 - 0xdc00);
                                } else {
                                    self.pos = save;
                                }
                            }
                            let ch = char::from_u32(r).unwrap_or('\u{fffd}');
                            out.extend_from_slice(ch.to_string().as_bytes());
                        }
                        _ => return Err(invalid(e, "in string escape code")),
                    }
                }
                c if c < 0x20 => return Err(invalid(c, "in string literal")),
                c => out.push(c),
            }
        }
    }
}

/// Go's `json.Unmarshal` syntax check and parse of a whole document.
pub(crate) fn parse(text: &[u8]) -> Result<JsonValue, String> {
    let mut p = Parser { s: text, pos: 0 };
    let v = p.value()?;
    p.skip_space();
    if p.peek().is_some() {
        return Err(p.invalid("after top-level value"));
    }
    Ok(v)
}

/// Go's `json.Marshal` of a string: quote, backslash and control characters escaped, the HTML characters and
/// U+2028 and U+2029 as `\u` escapes, and each byte of invalid UTF-8 as the escaped replacement character.
pub(crate) fn encode_string(s: &[u8]) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for chunk in s.utf8_chunks() {
        for c in chunk.valid().chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                    out.push_str(&format!("\\u{:04x}", c as u32))
                }
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        chunk.invalid().iter().for_each(|_| out.push_str("\\ufffd"));
    }
    out.push('"');
    out
}
