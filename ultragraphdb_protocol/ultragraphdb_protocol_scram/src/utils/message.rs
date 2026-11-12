/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::errors::scram_error::ScramError;

pub(crate) fn encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

pub(crate) fn decode(text: &str, part: &'static str) -> Result<Vec<u8>, ScramError> {
    STANDARD
        .decode(text)
        .map_err(|_| ScramError::malformed(part))
}

/// Whether `nonce` is a nonce of RFC 5802: printable ASCII but the comma, at least one character.
pub(crate) fn valid_nonce(nonce: &str) -> bool {
    !nonce.is_empty()
        && nonce
            .bytes()
            .all(|b| (0x21..=0x7E).contains(&b) && b != b',')
}

/// A name as a message carries it: `=` as `=3D` and `,` as `=2C`.
pub(crate) fn escape(name: &str) -> String {
    name.replace('=', "=3D").replace(',', "=2C")
}

/// A name a message carries, its escapes read; refused as [`ScramError::malformed`] for an `=` of no escape.
pub(crate) fn unescape(name: &str) -> Result<String, ScramError> {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some((before, after)) = rest.split_once('=') {
        out.push_str(before);
        let (escaped, tail) = match after.split_at_checked(2) {
            Some(("2C", tail)) => (',', tail),
            Some(("3D", tail)) => ('=', tail),
            _ => return Err(ScramError::malformed("name")),
        };
        out.push(escaped);
        rest = tail;
    }
    out.push_str(rest);
    Ok(out)
}

/// The value of attribute `key` in `part`, `key=value`; `None` for another.
pub(crate) fn attribute(part: Option<&str>, key: char) -> Option<&str> {
    let part = part?;
    let mut chars = part.chars();
    (chars.next() == Some(key) && chars.next() == Some('=')).then(|| &part[2..])
}

/// The server's error `e=`, when `message` is one.
pub(crate) fn server_error(message: &str) -> Option<ScramError> {
    message.strip_prefix("e=").map(ScramError::server)
}
