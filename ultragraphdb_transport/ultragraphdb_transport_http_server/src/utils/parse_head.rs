/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::http_limits::HttpLimits;
use crate::types::parsed_head::{HttpHead, ParsedHead};

const END_OF_HEAD: &[u8] = b"\r\n\r\n";

/// What `bytes`, all that has arrived on a connection, ask, within `limits`. What follows a request's declared body is
/// ignored, since a connection carries one request.
///
/// A `Transfer-Encoding` is refused, and two `Content-Length` headers, so no framing can be read two ways.
#[must_use]
pub fn parse_head(bytes: &[u8], limits: &HttpLimits) -> ParsedHead {
    let end = bytes
        .windows(END_OF_HEAD.len())
        .position(|window| window == END_OF_HEAD);
    let Some(end) = end else {
        return if bytes.len() >= limits.head() {
            ParsedHead::Refused(431)
        } else {
            ParsedHead::Incomplete
        };
    };
    if end >= limits.head() {
        return ParsedHead::Refused(431);
    }
    let Some(head) = bytes
        .get(..end)
        .and_then(|head| std::str::from_utf8(head).ok())
    else {
        return ParsedHead::Refused(400);
    };
    let body_start = end + END_OF_HEAD.len();
    match read_head(head, body_start, limits) {
        Ok(read) if bytes.len() < body_start + read.body_len => ParsedHead::Incomplete,
        Ok(read) => ParsedHead::Complete(read),
        Err(status) => ParsedHead::Refused(status),
    }
}

/// The head `text`, its body starting at `body_start`.
fn read_head(text: &str, body_start: usize, limits: &HttpLimits) -> Result<HttpHead, u16> {
    let mut lines = text.split("\r\n");
    let line = lines.next().unwrap_or_default();
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(400);
    };
    let token = !method.is_empty() && method.bytes().all(|byte| byte.is_ascii_uppercase());
    if !token || !target.starts_with('/') || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(400);
    }
    let mut bearer = None;
    let mut length: Option<usize> = None;
    for header in lines {
        let (name, value) = header.split_once(':').ok_or(400u16)?;
        if name.is_empty() || name.contains([' ', '\t']) {
            return Err(400);
        }
        let value = value.trim_matches([' ', '\t']);
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(501);
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(400);
            }
            length = Some(value.parse().map_err(|_| 413u16)?);
        }
        if name.eq_ignore_ascii_case("authorization") {
            bearer = value.strip_prefix("Bearer ").map(str::to_string);
        }
    }
    let body_len = length.unwrap_or(0);
    if body_len > limits.body() {
        return Err(413);
    }
    Ok(HttpHead {
        method: method.to_string(),
        target: target.to_string(),
        bearer,
        body_start,
        body_len,
    })
}
