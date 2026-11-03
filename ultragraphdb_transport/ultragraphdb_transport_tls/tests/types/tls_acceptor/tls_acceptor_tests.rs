/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};
use std::path::PathBuf;

use rustls::ProtocolVersion;
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{
    CertificateChainFile, PrivateKeyFile, TestEcho, TestPeer, TestPki, TlsAcceptor, TlsError,
    TlsErrorEnum,
};

/// A file of `contents` named `name`, unique to this process, in the test's temporary directory.
fn file(name: &str, contents: &[u8]) -> TestResult<String> {
    let dir = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let path = dir.join(format!("acceptor-{}-{name}", std::process::id()));
    std::fs::write(&path, contents).context("a temporary file")?;
    Ok(path.to_str().context("a UTF-8 path")?.to_string())
}

/// The error building an acceptor from `chain` and `key` failed with.
fn refusal(chain: &str, key: &str) -> TestResult<TlsError> {
    match TlsAcceptor::from_pem_files(&CertificateChainFile::new(chain), &PrivateKeyFile::new(key))
    {
        Ok(_) => Err(TestError::new("the acceptor was built")),
        Err(error) => Ok(error),
    }
}

#[test]
fn test_an_acceptor_reads_its_chain_and_key_from_pem_files() -> TestResult {
    let pki = TestPki::new()?;
    let chain = file("chain.pem", pki.server_chain_pem().as_bytes())?;
    let key = file("key.pem", pki.server_key_pem().as_bytes())?;
    let acceptor = TlsAcceptor::from_pem_files(
        &CertificateChainFile::new(chain.as_str()),
        &PrivateKeyFile::new(key.as_str()),
    )?;
    let mut tls = pki.connector()?.connect(
        TestPeer::new(acceptor.session()?, TestEcho::default()),
        "localhost",
    )?;
    assert_eq!(
        tls.session().protocol_version(),
        Some(ProtocolVersion::TLSv1_3)
    );
    tls.write_all(b"ping\n").context("a write")?;
    let mut echoed = [0u8; 5];
    tls.read_exact(&mut echoed).context("the echo")?;
    assert_eq!(&echoed, b"ping\n");
    Ok(())
}

#[test]
fn test_a_file_that_cannot_be_read_is_refused_naming_it() -> TestResult {
    let pki = TestPki::new()?;
    let chain = file("readable-chain.pem", pki.server_chain_pem().as_bytes())?;
    let key = file("readable-key.pem", pki.server_key_pem().as_bytes())?;
    let missing = "/nonexistent/ultragraphdb/missing.pem";
    for (chain_file, key_file) in [(missing, key.as_str()), (chain.as_str(), missing)] {
        let error = refusal(chain_file, key_file)?;
        assert!(
            matches!(&error.0, TlsErrorEnum::CertificateUnreadable { path, reason }
                if path == missing && !reason.is_empty()),
            "{error:?}"
        );
    }
    Ok(())
}

#[test]
fn test_a_file_holding_no_certificate_or_no_key_is_refused_as_empty() -> TestResult {
    let pki = TestPki::new()?;
    let chain = file("only-chain.pem", pki.server_chain_pem().as_bytes())?;
    let key = file("only-key.pem", pki.server_key_pem().as_bytes())?;
    let empty = file("empty.pem", b"")?;
    // A chain file holding a key and no certificate, and a key file holding a certificate and no key.
    for (chain_file, key_file, named) in [
        (empty.as_str(), key.as_str(), empty.as_str()),
        (key.as_str(), key.as_str(), key.as_str()),
        (chain.as_str(), empty.as_str(), empty.as_str()),
        (chain.as_str(), chain.as_str(), chain.as_str()),
    ] {
        let error = refusal(chain_file, key_file)?;
        assert_eq!(
            error.0,
            TlsErrorEnum::CertificateEmpty(named.to_string()),
            "{chain_file} {key_file}"
        );
    }
    Ok(())
}

#[test]
fn test_a_key_matching_another_certificate_is_refused_naming_the_key_file() -> TestResult {
    let pki = TestPki::new()?;
    let chain = file("mismatch-chain.pem", pki.server_chain_pem().as_bytes())?;
    let other = file("mismatch-key.pem", pki.other_key_pem().as_bytes())?;
    assert_eq!(
        refusal(&chain, &other)?.0,
        TlsErrorEnum::KeyMismatch(other.clone())
    );
    // Another authority's server key is another key too.
    let foreign = file(
        "foreign-key.pem",
        TestPki::new()?.server_key_pem().as_bytes(),
    )?;
    assert_eq!(
        refusal(&chain, &foreign)?.0,
        TlsErrorEnum::KeyMismatch(foreign.clone())
    );
    Ok(())
}

#[test]
fn test_der_values_build_an_acceptor_and_an_empty_chain_is_refused() -> TestResult {
    let pki = TestPki::new()?;
    let key = || PrivateKeyDer::from_pem_slice(pki.server_key_pem().as_bytes());
    let chain: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(pki.server_chain_pem().as_bytes())
            .collect::<Result<_, _>>()
            .context("the chain")?;
    let built = TlsAcceptor::from_der(chain, key().context("the key")?, "the key")?;
    // The acceptor serves the chain: a client trusting the authority completes a handshake with it.
    let mut client = TestPeer::new(pki.connector()?.session("localhost")?, TestEcho::default());
    let version = built.accept(&mut client)?.session().protocol_version();
    assert_eq!(version, Some(ProtocolVersion::TLSv1_3));
    assert!(!client.failed());
    let Err(error) = TlsAcceptor::from_der(Vec::new(), key().context("the key")?, "the key") else {
        return Err(TestError::new("an empty chain built an acceptor"));
    };
    assert_eq!(
        error.0,
        TlsErrorEnum::CertificateEmpty("the key".to_string())
    );
    let garbage = vec![CertificateDer::from(b"not a certificate".to_vec())];
    let Err(unusable) = TlsAcceptor::from_der(garbage, key().context("the key")?, "the key") else {
        return Err(TestError::new(
            "bytes that are no certificate built an acceptor",
        ));
    };
    assert!(
        matches!(unusable.0, TlsErrorEnum::ConfigurationRefused(_)),
        "{unusable:?}"
    );
    Ok(())
}

#[test]
fn test_accept_completes_the_handshake_before_the_caller_reads() -> TestResult {
    let pki = TestPki::new()?;
    let mut client = TestPeer::new(pki.connector()?.session("localhost")?, TestEcho::default());
    let mut tls = pki.acceptor()?.accept(&mut client)?;
    assert!(!tls.session().is_handshaking());
    assert_eq!(
        tls.session().protocol_version(),
        Some(ProtocolVersion::TLSv1_3)
    );
    assert_eq!(tls.session().host(), "");
    // The server speaks first here; the client's application echoes it.
    tls.write_all(b"welcome\n").context("a write")?;
    let mut echoed = [0u8; 8];
    tls.read_exact(&mut echoed).context("the echo")?;
    assert_eq!(&echoed, b"welcome\n");
    assert!(!client.failed());
    Ok(())
}

#[test]
fn test_a_client_offering_tls_1_2_alone_is_refused_at_accept() -> TestResult {
    let pki = TestPki::new()?;
    let mut client = TestPeer::new(pki.client_tls12("localhost")?, TestEcho::default());
    let Err(error) = pki.acceptor()?.accept(&mut client) else {
        return Err(TestError::new("a TLS 1.2 client was accepted"));
    };
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, .. } if host.is_empty()),
        "{error:?}"
    );
    assert!(
        client.failed(),
        "the client received the protocol_version alert"
    );
    Ok(())
}

#[test]
fn test_an_acceptor_opens_a_server_session_each_time() -> TestResult {
    let acceptor = TestPki::new()?.acceptor()?;
    let (one, two) = (acceptor.session()?, acceptor.clone().session()?);
    assert!(one.is_handshaking() && two.is_handshaking());
    assert!(!one.wants_write(), "a server waits for the client hello");
    assert!(one.wants_read());
    Ok(())
}
