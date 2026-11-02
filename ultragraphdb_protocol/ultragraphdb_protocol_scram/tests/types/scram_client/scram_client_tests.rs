/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{MAX_ITERATIONS, ScramClient, ScramError};
use ultragraphdb_protocol_transport::ClientMechanism;
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::rfc7677::{
    CLIENT_FINAL, CLIENT_FIRST, CLIENT_NONCE, NAME, PASSWORD, SERVER_FINAL, SERVER_FIRST,
};

fn client() -> TestResult<ScramClient> {
    Ok(ScramClient::new(NAME, PASSWORD, CLIENT_NONCE)?)
}

#[test]
fn test_the_client_sends_rfc_7677s_messages() -> TestResult {
    let mut client = client()?;
    assert_eq!(client.first_message()?, CLIENT_FIRST);
    assert_eq!(client.final_message(SERVER_FIRST)?, CLIENT_FINAL);
    assert_eq!(client.verify(SERVER_FINAL), Ok(()));
    Ok(())
}

#[test]
fn test_the_client_is_the_transports_mechanism() -> TestResult {
    let mut client = client()?;
    assert_eq!(client.name(), "SCRAM-SHA-256");
    assert_eq!(client.start()?, CLIENT_FIRST);
    assert_eq!(client.answer(SERVER_FIRST)?, CLIENT_FINAL);
    assert_eq!(client.finish(SERVER_FINAL), Ok(()));
    Ok(())
}

#[test]
fn test_a_name_escapes_its_comma_and_equals_sign() -> TestResult {
    let mut client = ScramClient::new("a,b=c", PASSWORD, CLIENT_NONCE)?;
    assert_eq!(
        client.first_message()?,
        b"n,,n=a=2Cb=3Dc,r=rOprNGfwEbeRWgbNEkqO"
    );
    Ok(())
}

#[test]
fn test_a_name_password_or_nonce_the_rules_refuse() -> TestResult {
    assert_eq!(
        ScramClient::new("us\u{0007}er", PASSWORD, CLIENT_NONCE).err(),
        Some(ScramError::Prepare())
    );
    assert_eq!(
        ScramClient::new(NAME, "pen\u{0007}cil", CLIENT_NONCE).err(),
        Some(ScramError::Prepare())
    );
    for nonce in ["", "a,b", "caf\u{00e9}", "a b"] {
        assert_eq!(
            ScramClient::new(NAME, PASSWORD, nonce).err(),
            Some(ScramError::Malformed("nonce")),
            "{nonce:?}"
        );
    }
    Ok(())
}

#[test]
fn test_the_servers_first_message_is_checked() -> TestResult {
    let first = |server_first: &[u8]| -> TestResult<Result<Vec<u8>, ScramError>> {
        let mut client = client()?;
        assert_eq!(client.first_message()?, CLIENT_FIRST);
        Ok(client.final_message(server_first))
    };
    // A nonce that does not extend the client's.
    assert_eq!(
        first(b"r=other%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096")?,
        Err(ScramError::Nonce())
    );
    // A nonce that is the client's alone, with nothing of the server's.
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096")?,
        Err(ScramError::Nonce())
    );
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4095")?,
        Err(ScramError::Iterations(4_095))
    );
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO%hv,s=!!!,i=4096")?,
        Err(ScramError::Malformed("salt"))
    );
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=x")?,
        Err(ScramError::Malformed("iteration count"))
    );
    assert_eq!(first(b"nonsense")?, Err(ScramError::Malformed("nonce")));
    assert_eq!(
        first(b"e=unknown-user")?,
        Err(ScramError::Server("unknown-user"))
    );
    Ok(())
}

#[test]
fn test_the_servers_final_message_is_checked() -> TestResult {
    let finished = |server_final: &[u8]| -> TestResult<Result<(), ScramError>> {
        let mut client = client()?;
        assert_eq!(client.first_message()?, CLIENT_FIRST);
        assert_eq!(client.final_message(SERVER_FIRST)?, CLIENT_FINAL);
        Ok(client.verify(server_final))
    };
    assert_eq!(
        finished(b"v=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")?,
        Err(ScramError::ServerSignature())
    );
    assert_eq!(
        finished(b"e=invalid-proof")?,
        Err(ScramError::Server("invalid-proof"))
    );
    assert_eq!(finished(b"x=1")?, Err(ScramError::Malformed("verifier")));
    Ok(())
}

#[test]
fn test_messages_out_of_step_are_refused() -> TestResult {
    let mut client = client()?;
    assert_eq!(
        client.final_message(SERVER_FIRST),
        Err(ScramError::OutOfStep())
    );
    assert_eq!(client.verify(SERVER_FINAL), Err(ScramError::OutOfStep()));
    assert_eq!(client.first_message()?, CLIENT_FIRST);
    assert_eq!(client.first_message(), Err(ScramError::OutOfStep()));
    assert_eq!(client.verify(SERVER_FINAL), Err(ScramError::OutOfStep()));
    Ok(())
}

#[test]
fn test_more_iterations_than_the_most_are_refused_before_any_is_run() -> TestResult {
    // A server naming 2^32 - 1 would hold the client for hours (SECURITY.md); the count is refused before the key is
    // derived, so neither refusal takes a moment.
    assert_eq!(MAX_ITERATIONS, 10_000_000);
    for count in [MAX_ITERATIONS + 1, u32::MAX] {
        let mut client = client()?;
        assert_eq!(client.first_message()?, CLIENT_FIRST);
        let server_first =
            format!("r=rOprNGfwEbeRWgbNEkqO%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i={count}");
        assert_eq!(
            client.final_message(server_first.as_bytes()),
            Err(ScramError::Iterations(count))
        );
    }
    Ok(())
}

#[test]
fn test_a_server_nonce_extending_the_clients_with_a_character_outside_the_rules_is_refused()
-> TestResult {
    // RFC 5802 section 5.1: a nonce is printable ASCII but the comma; the space, 0x20, is outside it, so the server's
    // nonce is refused though it extends the client's.
    let mut client = client()?;
    assert_eq!(client.first_message()?, CLIENT_FIRST);
    assert_eq!(
        client.final_message(b"r=rOprNGfwEbeRWgbNEkqO h,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096"),
        Err(ScramError::Nonce())
    );
    Ok(())
}

#[test]
fn test_the_mechanisms_finish_refuses_a_wrong_server_signature() -> TestResult {
    // RFC 7677's exchange, but a server-final-message whose signature is all zero bytes: not the one the password's
    // server key gives, so the transport's mechanism fails the exchange as `verify` does.
    let mut client = client()?;
    assert_eq!(client.start()?, CLIENT_FIRST);
    assert_eq!(client.answer(SERVER_FIRST)?, CLIENT_FINAL);
    assert_eq!(
        client.finish(b"v=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="),
        Err(ScramError::ServerSignature())
    );
    Ok(())
}
