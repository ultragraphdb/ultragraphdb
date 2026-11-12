/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

// Arithmetic on decoded wire input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

use crate::types::http_reply::{HttpReply, ParsedReply};

const END_OF_HEAD: &[u8] = b"\r\n\r\n";

/// The most a status line and its headers take.
const HEAD_MAX: usize = 16 * 1024;

/// What `bytes`, all that has arrived of a reply, come to, its body at most `body_max` bytes. A reply states its body's
/// length, as the control plane's server always does, and what follows that body is ignored.
#[must_use]
pub fn parse_reply(bytes: &[u8], body_max: usize) -> ParsedReply {
    let wrong = |why: &str| ParsedReply::Malformed(why.to_string());
    let end = bytes
        .windows(END_OF_HEAD.len())
        .position(|window| window == END_OF_HEAD);
    let Some(end) = end else {
        return if bytes.len() >= HEAD_MAX {
            wrong("a head past its bound")
        } else {
            ParsedReply::Incomplete
        };
    };
    if end >= HEAD_MAX {
        return wrong("a head past its bound");
    }
    let Some(head) = bytes
        .get(..end)
        .and_then(|head| std::str::from_utf8(head).ok())
    else {
        return wrong("a head that is not text");
    };
    let mut lines = head.split("\r\n");
    let line = lines.next().unwrap_or_default();
    let mut parts = line.splitn(3, ' ');
    let (Some(version), Some(status)) = (parts.next(), parts.next()) else {
        return wrong("no status line");
    };
    if !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return wrong("no HTTP version");
    }
    let Some(status) = status
        .parse::<u16>()
        .ok()
        .filter(|status| (100..=999).contains(status))
    else {
        return wrong("no status");
    };
    let mut headers = Vec::new();
    for header in lines {
        let Some((name, value)) = header.split_once(':') else {
            return wrong("a header with no colon");
        };
        headers.push((
            name.to_string(),
            value.trim_matches([' ', '\t']).to_string(),
        ));
    }
    let mut lengths = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"));
    let (Some((_, length)), None) = (lengths.next(), lengths.next()) else {
        return wrong("not one Content-Length");
    };
    if !length.bytes().all(|byte| byte.is_ascii_digit()) {
        return wrong("a Content-Length that is no number");
    }
    let Some(length) = length.parse::<usize>().ok().filter(|len| *len <= body_max) else {
        return wrong("a body past its bound");
    };
    // `end` is where the window `END_OF_HEAD` was found, so the body follows it.
    let body = bytes
        .get(end..)
        .and_then(|rest| rest.strip_prefix(END_OF_HEAD))
        .and_then(|rest| rest.get(..length));
    match body {
        Some(body) => ParsedReply::Complete(HttpReply {
            status,
            headers,
            body: body.to_vec(),
        }),
        None => ParsedReply::Incomplete,
    }
}
