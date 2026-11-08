/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_http_server::{BearerKey, HttpServerErrorEnum};

const KEY: &str = "0123456789abcdef0123456789abcdef";
/// SHA-256 of `KEY`, by `shasum -a 256`.
const DIGEST: &str = "3eb1bd439947eb762998e566ccc2e099c791118b2f40579cc4f7da2b5061b7f9";

fn holding(name: &str, bytes: &[u8]) -> TestResult<PathBuf> {
    let file = std::env::temp_dir().join(format!("bearer-{name}-{}", std::process::id()));
    std::fs::write(&file, bytes)?;
    Ok(file)
}

/// A key from its file allows itself alone, its final line break no part of it.
#[test]
fn test_a_key_allows_itself_alone() -> TestResult {
    for (name, bytes) in [
        ("bare", KEY.to_string()),
        ("newline", format!("{KEY}\n")),
        ("crlf", format!("{KEY}\r\n")),
    ] {
        let file = holding(name, bytes.as_bytes())?;
        let key = BearerKey::read(&file.display().to_string())?;
        assert!(key.allows(Some(KEY)), "{name}");
        assert!(
            !key.allows(Some("0123456789abcdef0123456789abcdeX")),
            "{name}"
        );
        assert!(!key.allows(Some("")), "{name}");
        assert!(!key.allows(None), "{name}");
    }
    Ok(())
}

/// A digest verifies the key whose digest it is, as a key read from its file does.
#[test]
fn test_a_digest_verifies_its_key() -> TestResult {
    let key = BearerKey::from_digest_hex(DIGEST)?;
    assert!(key.allows(Some(KEY)));
    assert!(!key.allows(Some("another")));
    let upper = BearerKey::from_digest_hex(&DIGEST.to_ascii_uppercase())?;
    assert!(upper.allows(Some(KEY)));
    for wrong in ["", "abc", &DIGEST[1..], &format!("z{}", &DIGEST[1..])] {
        let refused = BearerKey::from_digest_hex(wrong)
            .err()
            .context("a refusal")?;
        assert_eq!(refused.0, HttpServerErrorEnum::DigestMalformed, "{wrong}");
        assert!(refused.to_string().contains(&DIGEST.len().to_string()));
    }
    Ok(())
}

/// A key too short, or a file that cannot be read, is refused naming the file; 32 bytes is the first accepted.
#[test]
fn test_a_key_too_short_names_its_file() -> TestResult {
    let short = holding("short", &[b'a'; 31])?;
    let refused = BearerKey::read(&short.display().to_string())
        .err()
        .context("a refusal")?;
    assert!(matches!(
        refused.0,
        HttpServerErrorEnum::KeyTooShort {
            bytes: 31,
            required: 32,
            ..
        }
    ));
    assert!(refused.to_string().contains(&short.display().to_string()));
    let exact = holding("exact", &[b'a'; 32])?;
    assert!(BearerKey::read(&exact.display().to_string()).is_ok());
    let absent = "/nonexistent/ultragraphdb/key";
    let unread = BearerKey::read(absent).err().context("a refusal")?;
    assert!(matches!(
        unread.0,
        HttpServerErrorEnum::KeyUnreadable { .. }
    ));
    assert!(unread.to_string().contains(absent));
    Ok(())
}

/// A key's `Debug` holds nothing of it.
#[test]
fn test_a_key_never_prints() -> TestResult {
    let key = BearerKey::from_digest_hex(DIGEST)?;
    assert_eq!(format!("{key:?}"), "BearerKey");
    Ok(())
}
