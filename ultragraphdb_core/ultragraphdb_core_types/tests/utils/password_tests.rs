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

#[test]
fn test_encrypt() {
    let cases = [
        ("", 0, true),
        ("12345", 0, true),
        ("123456", 60, false),
        ("1234567890", 60, false),
    ];
    for (input, want, want_err) in cases {
        let got = encrypt(input);
        assert_eq!(got.is_err(), want_err, "{input}");
        match got {
            Err(e) => assert_eq!(
                e.to_string(),
                "Password too short, i.e. should have at least 6 chars",
                "{input}"
            ),
            Ok(h) => {
                assert_eq!(h.len(), want, "{input}");
                assert!(h.starts_with("$2a$10$"), "{h}");
                verify_password(input, &h).unwrap();
            }
        }
    }
}

/// golang.org/x/crypto/bcrypt v0.56.0 refuses to hash more than 72 bytes; compared, a longer password is read as
/// its first 72 bytes.
#[test]
fn test_encrypt_length_limit() {
    let at_limit = "x".repeat(72);
    let h = encrypt(&at_limit).unwrap();
    assert_eq!(h.len(), 60);
    assert_eq!(
        encrypt("x".repeat(73)).unwrap_err().to_string(),
        "bcrypt: password length exceeds 72 bytes"
    );
    verify_password("x".repeat(73), &h).unwrap();
    assert_eq!(
        verify_password("x".repeat(71), &h).unwrap_err().to_string(),
        "crypto/bcrypt: hashedPassword is not the hash of the given password"
    );
}

#[test]
fn test_verify_password() {
    let cases = [
        ("", "", true),
        (
            "",
            "$2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
            true,
        ),
        ("1234567890", "", true),
        (
            "12345",
            "$2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
            true,
        ),
        (
            "12345678",
            "$2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
            true,
        ),
        (
            "123456",
            "$2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC",
            false,
        ),
        (
            "123456",
            "$2a$10$kXtSFCVmzsu0lwVqaWxo5OXlLGUakcY2t18QcqcVpvoTHsPqclOca",
            false,
        ),
        (
            "1234567890",
            "$2a$10$WdCWNpOP6c4l7ECv3hEWKeD3oSiszlRJFmT4uRT1P/W9V9zUye8pS",
            false,
        ),
    ];
    for (plain, hash, want_err) in cases {
        let got = verify_password(plain, hash);
        assert_eq!(got.is_err(), want_err, "{plain} {hash}");
        if let Err(e) = got {
            let want = if plain.len() < 6 || hash.is_empty() {
                "Invalid password/crypted string"
            } else {
                "crypto/bcrypt: hashedPassword is not the hash of the given password"
            };
            assert_eq!(e.to_string(), want, "{plain} {hash}");
        }
    }
}

/// golang.org/x/crypto/bcrypt v0.56.0's `CompareHashAndPassword` on malformed hashes of "123456", as Go answers
/// them: the version byte is only bounded above, any minor version or none is accepted, the cost is read with
/// `strconv.Atoi` and range-checked, the salt is base64-decoded, and bytes past the 60th are ignored.
#[test]
fn test_verify_password_malformed_hashes() {
    const GOOD: &str = "$2a$10$LMGgvb5dOq4/YrWXjAy6W.tBfFQC4QDNFAuOCWGRk3f/Z1TMXswaC";
    let rest = &GOOD[7..];
    let cases: Vec<(String, &str)> = vec![
        (GOOD.into(), ""),
        (format!("$2b$10${rest}"), ""),
        (format!("$2y$10${rest}"), ""),
        (format!("$2x$10${rest}"), ""),
        (format!("$1a$10${rest}"), ""),
        (format!("$2$10${rest}"), ""),
        (format!("{GOOD}xyz"), ""),
        (
            format!("$2a$03${rest}"),
            "crypto/bcrypt: cost 3 is outside allowed inclusive range 4..31",
        ),
        (
            format!("$2a$32${rest}"),
            "crypto/bcrypt: cost 32 is outside allowed inclusive range 4..31",
        ),
        (
            format!("$2a$-1${rest}"),
            "crypto/bcrypt: cost -1 is outside allowed inclusive range 4..31",
        ),
        (
            format!("$2a$1x${rest}"),
            "strconv.Atoi: parsing \"1x\": invalid syntax",
        ),
        (
            format!("$2a$10$!{}", &rest[1..]),
            "illegal base64 data at input byte 0",
        ),
        (
            format!("{}!", &GOOD[..59]),
            "crypto/bcrypt: hashedPassword is not the hash of the given password",
        ),
        (
            GOOD[..59].to_string(),
            "crypto/bcrypt: hashedPassword is not the hash of the given password",
        ),
        (
            format!("x{}", &GOOD[1..]),
            "crypto/bcrypt: bcrypt hashes must start with '$', but hashedSecret started with 'x'",
        ),
        (
            format!("$3a$10${rest}"),
            "crypto/bcrypt: bcrypt algorithm version '3' requested is newer than current version '2'",
        ),
        (
            GOOD[..58].into(),
            "crypto/bcrypt: hashedSecret too short to be a bcrypted password",
        ),
        // The bytes after the version and after the cost are skipped unread; Atoi takes a sign.
        (format!("$2a?10?{rest}"), ""),
        (
            format!("$2a$+1${rest}"),
            "crypto/bcrypt: cost 1 is outside allowed inclusive range 4..31",
        ),
    ];
    // Go's base64 decoder on the salt, which x/crypto pads with "==": misplaced padding, line breaks it skips and
    // bytes outside the alphabet.
    let (salt, hash) = (&GOOD[7..29], &GOOD[29..]);
    let salted = |s: String| format!("$2a$10${s}{hash}");
    let corrupt = |i: usize| format!("illegal base64 data at input byte {i}");
    let mismatch = "crypto/bcrypt: hashedPassword is not the hash of the given password";
    let salts: Vec<(String, String)> = vec![
        (format!("{}={}", &salt[..2], &salt[3..]), corrupt(2)),
        (format!("{}={}", &salt[..3], &salt[4..]), corrupt(4)),
        (format!("{}=={}", &salt[..2], &salt[4..]), corrupt(4)),
        (format!("\n{}", &salt[1..]), corrupt(22)),
        (format!("{}=", &salt[..21]), corrupt(21)),
        (format!("{}==", &salt[..20]), corrupt(20)),
        (
            format!("{}\r\n\n\n{}", &salt[..4], &salt[8..]),
            mismatch.into(),
        ),
        (format!("{}\u{e9}", &salt[..20]), corrupt(20)),
    ];
    let cases: Vec<(String, String)> = cases
        .into_iter()
        .map(|(h, w)| (h, w.to_string()))
        .chain(salts.into_iter().map(|(s, w)| (salted(s), w)))
        .collect();
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(hash, want)| {
            let got = verify_password("123456", hash)
                .err()
                .map(|e| e.to_string())
                .unwrap_or_default();
            (got != *want).then(|| format!("{hash}: got {got:?}, Go {want:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Deviation: x/crypto v0.56.0 accepts this hash, whose salt holds four line breaks and decodes to 13 bytes, because
/// its hash part was computed with that 13-byte salt (probed with Go 1.27). The port takes 16-byte salts only and
/// reports a mismatch.
#[test]
fn test_verify_password_short_salt_deviation() {
    assert_eq!(
        verify_password(
            "123456",
            "$2a$10$LMGg\r\n\n\nOq4/YrWXjAy6W.mpHTlrSY83rd7rKFKeChhiM0T8c8GSC"
        )
        .unwrap_err()
        .to_string(),
        "crypto/bcrypt: hashedPassword is not the hash of the given password"
    );
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
fn test_golden_go_written_hashes() {
    let path = ultragraphdb_tool_bdd::testdata_dir().join("golden/types_password.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<PasswordFile>(&path);
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
                verify_password(&e.plain, &h).unwrap();
            }
        }
    }
    for v in &f.verify {
        let got = verify_password(&v.plain, &v.hash)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
        assert_eq!(got, v.err, "{} {}", v.plain, v.hash);
    }
}
