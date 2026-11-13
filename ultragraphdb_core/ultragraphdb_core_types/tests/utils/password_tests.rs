/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/password_test.go. Changed for ultragraphdb: translated to Rust; plus the
 * Go-written hashes (tools/golden/types.go, types_password).
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_types::{encrypt, verify_password};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A bcrypt hash of "123456".
const GOOD: &str = "$2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC";
const MISMATCH: &str = "bcrypt: the password does not match its hash";

#[test]
fn test_encrypt() -> TestResult {
    for short in ["", "12345"] {
        assert_eq!(
            encrypt(short).err().context("a refusal")?.to_string(),
            "Password too short, i.e. should have at least 6 chars",
            "{short}"
        );
    }
    for plain in ["123456", "1234567890"] {
        let hash = encrypt(plain)?;
        assert_eq!(hash.len(), GOOD.len(), "{plain}");
        assert!(hash.starts_with("$2a$10$"), "{hash}");
        verify_password(plain, &hash)?;
        assert_ne!(encrypt(plain)?, hash, "a salt of its own each time");
    }
    Ok(())
}

/// A password of more than 72 bytes is refused, hashed or compared: bcrypt reads 72 and nothing is cut.
#[test]
fn test_the_length_limit() -> TestResult {
    let at_limit = "x".repeat(72);
    let hash = encrypt(&at_limit)?;
    verify_password(&at_limit, &hash)?;
    assert_eq!(
        encrypt("x".repeat(73))
            .err()
            .context("a refusal")?
            .to_string(),
        "bcrypt: password length exceeds 72 bytes"
    );
    assert_eq!(
        verify_password("x".repeat(73), &hash)
            .err()
            .context("a refusal")?
            .to_string(),
        "bcrypt: password length exceeds 72 bytes"
    );
    assert_eq!(
        verify_password("x".repeat(71), &hash)
            .err()
            .context("a refusal")?
            .to_string(),
        MISMATCH
    );
    Ok(())
}

#[test]
fn test_verify_password() -> TestResult {
    verify_password("123456", GOOD)?;
    verify_password(
        "123456",
        "$2a$10$kXtSFCVmzsu0lwVqaWxo5OXlLGUakcY2t18QcqcVpvoTHsPqclOca",
    )?;
    verify_password(
        "1234567890",
        "$2a$10$WdCWNpOP6c4l7ECv3hEWKeD3oSiszlRJFmT4uRT1P/W9V9zUye8pS",
    )?;
    for (plain, hash, want) in [
        ("", "", "Invalid password/crypted string"),
        ("", GOOD, "Invalid password/crypted string"),
        ("1234567890", "", "Invalid password/crypted string"),
        ("12345", GOOD, "Invalid password/crypted string"),
        ("12345678", GOOD, MISMATCH),
    ] {
        assert_eq!(
            verify_password(plain, hash)
                .err()
                .context("a refusal")?
                .to_string(),
            want,
            "{plain} {hash}"
        );
    }
    Ok(())
}

/// A hash that is no bcrypt hash is refused as one, never as a mismatch that reads like a wrong password.
#[test]
fn test_a_malformed_hash_is_refused() -> TestResult {
    let cut = GOOD.get(..40).context("a prefix")?;
    let malformed: [&[u8]; 8] = [
        b"x",
        cut.as_bytes(),
        b"x2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
        b"$2a$03$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
        b"$2a$32$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
        b"$2a$xx$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
        b"$2a$10$!MGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
        b"$2a$10$\xffMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
    ];
    for hash in malformed {
        let refusal = verify_password("123456", hash)
            .err()
            .context("a refusal")?
            .to_string();
        assert!(refusal.starts_with("bcrypt: "), "{refusal}");
        assert_ne!(refusal, MISMATCH, "{hash:?}");
    }
    Ok(())
}

#[derive(Deserialize)]
struct EncryptCase {
    plain: String,
    #[serde(default)]
    hash: String,
    #[serde(default)]
    err: String,
}

#[derive(Deserialize)]
struct VerifyCase {
    plain: String,
    hash: String,
    #[serde(default)]
    err: String,
}

#[derive(Deserialize)]
struct PasswordFile {
    encrypt: Vec<EncryptCase>,
    verify: Vec<VerifyCase>,
}

#[test]
fn test_golden_go_written_hashes() -> TestResult {
    let path = ultragraphdb_tool_bdd::testdata_dir()?.join("golden/types_password.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<PasswordFile>(&path)?;
    let f = &golden.cases()[0];
    for e in &f.encrypt {
        match encrypt(&e.plain) {
            Err(err) => assert_eq!(err.to_string(), e.err),
            Ok(h) => {
                assert!(
                    e.err.is_empty() && h.starts_with("$2a$10$") && h.len() == e.hash.len(),
                    "{}",
                    e.plain
                );
                verify_password(&e.plain, &h)?;
            }
        }
    }
    // What Go's bcrypt accepts and refuses, the bcrypt crate does: a hash Go wrote verifies its password alone.
    for v in &f.verify {
        let got = verify_password(&v.plain, &v.hash);
        assert_eq!(
            got.is_err(),
            !v.err.is_empty(),
            "{} {}: {got:?}",
            v.plain,
            v.hash
        );
    }
    Ok(())
}
