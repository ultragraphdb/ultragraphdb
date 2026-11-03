/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{
    CA_BUNDLE_KEY, DEFAULT_CA_BUNDLE, TestPki, TlsError, TlsErrorEnum, TlsTrust,
};

/// A block in certificate armour whose content is no certificate: the base64 of `not a certificate`.
const NOT_A_CERTIFICATE: &str =
    "-----BEGIN CERTIFICATE-----\nbm90IGEgY2VydGlmaWNhdGU=\n-----END CERTIFICATE-----\n";

/// A private-key block, which a trust store skips: the base64 of `not a key`.
const A_KEY_BLOCK: &str = "-----BEGIN PRIVATE KEY-----\nbm90IGEga2V5\n-----END PRIVATE KEY-----\n";

/// A file of `contents` named for `name` in the test's own temporary directory: Bazel's `TEST_TMPDIR`, else the
/// system's.
fn temp_file(name: &str, contents: &[u8]) -> TestResult<String> {
    let dir = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!("{name}-{}.pem", std::process::id()));
    std::fs::write(&path, contents).context("a temporary file")?;
    Ok(path.to_str().context("a UTF-8 path")?.to_string())
}

/// The error `result` holds, or a [`TestError`] naming what was `expected` to fail.
fn refused(result: Result<TlsTrust, TlsError>, expected: &str) -> TestResult<TlsError> {
    let Err(error) = result else {
        return Err(TestError::new(format!("{expected}: read as a trust store")));
    };
    Ok(error)
}

#[test]
fn test_a_pem_file_yields_its_certificates_each_once() -> TestResult {
    let (one, two) = (TestPki::new()?, TestPki::new()?);
    let path = temp_file(
        "bundle",
        format!("{}{}{}", one.ca_pem(), two.ca_pem(), one.ca_pem()).as_bytes(),
    )?;
    let trust = TlsTrust::from_pem_file(&path).context("two certificates")?;
    assert_eq!(trust.len(), 2, "the repeated anchor is kept once");
    assert!(!trust.is_empty());
    assert_eq!(trust.source(), path);
    Ok(())
}

#[test]
fn test_one_certificate_is_one_anchor() -> TestResult {
    let pki = TestPki::new()?;
    let trust = TlsTrust::from_pem("one", pki.ca_pem().as_bytes()).context("one certificate")?;
    assert_eq!((trust.len(), trust.source()), (1, "one"));
    Ok(())
}

#[test]
fn test_unusable_blocks_are_skipped_and_the_certificates_kept() -> TestResult {
    let (one, two) = (TestPki::new()?, TestPki::new()?);
    let pem = format!(
        "{NOT_A_CERTIFICATE}{}{A_KEY_BLOCK}{}",
        one.ca_pem(),
        two.ca_pem()
    );
    let trust = TlsTrust::from_pem("mixed", pem.as_bytes()).context("two usable")?;
    assert_eq!(trust.len(), 2);
    Ok(())
}

#[test]
fn test_a_source_without_a_usable_certificate_is_refused() -> TestResult {
    for pem in ["", "not pem at all", NOT_A_CERTIFICATE, A_KEY_BLOCK] {
        let error = refused(TlsTrust::from_pem("empty", pem.as_bytes()), pem)?;
        assert_eq!(
            error.0,
            TlsErrorEnum::TrustStoreEmpty("empty".to_string()),
            "{pem:?}"
        );
    }
    Ok(())
}

#[test]
fn test_a_file_without_a_usable_certificate_is_refused_with_its_path() -> TestResult {
    let path = temp_file("empty", NOT_A_CERTIFICATE.as_bytes())?;
    let error = refused(TlsTrust::from_pem_file(&path), "nothing usable")?;
    assert_eq!(error.0, TlsErrorEnum::TrustStoreEmpty(path));
    Ok(())
}

#[test]
fn test_a_missing_file_is_unreadable_with_its_path() -> TestResult {
    let error = refused(
        TlsTrust::from_pem_file("/nonexistent/ultragraphdb/ca.pem"),
        "no file",
    )?;
    assert!(
        matches!(&error.0, TlsErrorEnum::TrustStoreUnreadable { path, reason }
            if path == "/nonexistent/ultragraphdb/ca.pem" && !reason.is_empty()),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn test_the_system_bundle_is_the_file_ssl_cert_file_names() -> TestResult {
    let pki = TestPki::new()?;
    let named = temp_file("ssl-cert-file", pki.ca_pem().as_bytes())?;
    let trust = TlsTrust::system_from(Some(&named)).context("the named bundle")?;
    assert_eq!((trust.len(), trust.source()), (1, named.as_str()));
    let missing = refused(
        TlsTrust::system_from(Some("/nonexistent/ultragraphdb/named.pem")),
        "no file",
    )?;
    assert!(
        matches!(&missing.0, TlsErrorEnum::TrustStoreUnreadable { path, .. } if path == "/nonexistent/ultragraphdb/named.pem"),
        "{missing:?}"
    );
    Ok(())
}

/// The path a trust store read from or failed on.
fn path_of(result: Result<TlsTrust, TlsError>) -> TestResult<String> {
    match result {
        Ok(trust) => Ok(trust.source().to_string()),
        Err(error) => {
            let path = match &error.0 {
                TlsErrorEnum::TrustStoreUnreadable { path, .. } => Some(path.clone()),
                TlsErrorEnum::TrustStoreEmpty(source) => Some(source.clone()),
                TlsErrorEnum::InvalidServerName(_)
                | TlsErrorEnum::HandshakeFailed { .. }
                | TlsErrorEnum::ConfigurationRefused(_)
                | TlsErrorEnum::CertificateUnreadable { .. }
                | TlsErrorEnum::CertificateEmpty(_)
                | TlsErrorEnum::KeyMismatch(_)
                | TlsErrorEnum::RecordRefused { .. } => None,
            };
            path.context(&format!(
                "a trust store's failure names its source: {error:?}"
            ))
        }
    }
}

#[test]
fn test_an_unset_or_empty_ssl_cert_file_means_the_default_bundle() -> TestResult {
    // Whether this host ships the default bundle varies; either outcome names the default path.
    assert_eq!(path_of(TlsTrust::system_from(None))?, DEFAULT_CA_BUNDLE);
    assert_eq!(path_of(TlsTrust::system_from(Some("")))?, DEFAULT_CA_BUNDLE);
    assert_eq!(DEFAULT_CA_BUNDLE, "/etc/ssl/certs/ca-certificates.crt");
    Ok(())
}

#[test]
fn test_system_reads_ssl_cert_file_from_the_environment() -> TestResult {
    // The variable is read, not set: the test process's own value, whatever it is, decides the path.
    assert_eq!(CA_BUNDLE_KEY, "SSL_CERT_FILE");
    let named = std::env::var(CA_BUNDLE_KEY).ok();
    let expected = named
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_CA_BUNDLE);
    assert_eq!(path_of(TlsTrust::system())?, expected);
    Ok(())
}
