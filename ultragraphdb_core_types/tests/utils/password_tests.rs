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
        assert_eq!(got.map(|h| h.len()).unwrap_or(0), want, "{input}");
    }
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
        assert_eq!(
            verify_password(plain, hash).is_err(),
            want_err,
            "{plain} {hash}"
        );
    }
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
    let path = ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_types")
        .join("golden/types_password.json");
    let golden = ultragraphdb_test_bdd::load_golden::<PasswordFile>(&path);
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
