/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{MAX_ITERATIONS, ScramClient, ScramError};
use ultragraphdb_protocol_transport::ClientMechanism;

use crate::utils::rfc7677::{
    CLIENT_FINAL, CLIENT_FIRST, CLIENT_NONCE, NAME, PASSWORD, SERVER_FINAL, SERVER_FIRST,
};

fn client() -> ScramClient {
    ScramClient::new(NAME, PASSWORD, CLIENT_NONCE).unwrap()
}

#[test]
fn test_the_client_sends_rfc_7677s_messages() {
    let mut client = client();
    assert_eq!(client.first_message().unwrap(), CLIENT_FIRST);
    assert_eq!(client.final_message(SERVER_FIRST).unwrap(), CLIENT_FINAL);
    assert_eq!(client.verify(SERVER_FINAL), Ok(()));
}

#[test]
fn test_the_client_is_the_transports_mechanism() {
    let mut client = client();
    assert_eq!(client.name(), "SCRAM-SHA-256");
    assert_eq!(client.start().unwrap(), CLIENT_FIRST);
    assert_eq!(client.answer(SERVER_FIRST).unwrap(), CLIENT_FINAL);
    assert_eq!(client.finish(SERVER_FINAL), Ok(()));
}

#[test]
fn test_a_name_escapes_its_comma_and_equals_sign() {
    let mut client = ScramClient::new("a,b=c", PASSWORD, CLIENT_NONCE).unwrap();
    assert_eq!(
        client.first_message().unwrap(),
        b"n,,n=a=2Cb=3Dc,r=rOprNGfwEbeRWgbNEkqO"
    );
}

#[test]
fn test_a_name_password_or_nonce_the_rules_refuse() {
    assert_eq!(
        ScramClient::new("us\u{0007}er", PASSWORD, CLIENT_NONCE).unwrap_err(),
        ScramError::Prepare()
    );
    assert_eq!(
        ScramClient::new(NAME, "pen\u{0007}cil", CLIENT_NONCE).unwrap_err(),
        ScramError::Prepare()
    );
    for nonce in ["", "a,b", "caf\u{00e9}", "a b"] {
        assert_eq!(
            ScramClient::new(NAME, PASSWORD, nonce).unwrap_err(),
            ScramError::Malformed("nonce"),
            "{nonce:?}"
        );
    }
}

#[test]
fn test_the_servers_first_message_is_checked() {
    let first = |server_first: &[u8]| {
        let mut client = client();
        client.first_message().unwrap();
        client.final_message(server_first)
    };
    // A nonce that does not extend the client's.
    assert_eq!(
        first(b"r=other%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096"),
        Err(ScramError::Nonce())
    );
    // A nonce that is the client's alone, with nothing of the server's.
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096"),
        Err(ScramError::Nonce())
    );
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4095"),
        Err(ScramError::Iterations(4_095))
    );
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO%hv,s=!!!,i=4096"),
        Err(ScramError::Malformed("salt"))
    );
    assert_eq!(
        first(b"r=rOprNGfwEbeRWgbNEkqO%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=x"),
        Err(ScramError::Malformed("iteration count"))
    );
    assert_eq!(first(b"nonsense"), Err(ScramError::Malformed("nonce")));
    assert_eq!(
        first(b"e=unknown-user"),
        Err(ScramError::Server("unknown-user"))
    );
}

#[test]
fn test_the_servers_final_message_is_checked() {
    let finished = |server_final: &[u8]| {
        let mut client = client();
        client.first_message().unwrap();
        client.final_message(SERVER_FIRST).unwrap();
        client.verify(server_final)
    };
    assert_eq!(
        finished(b"v=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="),
        Err(ScramError::ServerSignature())
    );
    assert_eq!(
        finished(b"e=invalid-proof"),
        Err(ScramError::Server("invalid-proof"))
    );
    assert_eq!(finished(b"x=1"), Err(ScramError::Malformed("verifier")));
}

#[test]
fn test_messages_out_of_step_are_refused() {
    let mut client = client();
    assert_eq!(
        client.final_message(SERVER_FIRST),
        Err(ScramError::OutOfStep())
    );
    assert_eq!(client.verify(SERVER_FINAL), Err(ScramError::OutOfStep()));
    client.first_message().unwrap();
    assert_eq!(client.first_message(), Err(ScramError::OutOfStep()));
    assert_eq!(client.verify(SERVER_FINAL), Err(ScramError::OutOfStep()));
}

#[test]
fn test_more_iterations_than_the_most_are_refused_before_any_is_run() {
    // A server naming 2^32 - 1 would hold the client for hours (SECURITY.md); the count is refused before the key is
    // derived, so neither refusal takes a moment.
    assert_eq!(MAX_ITERATIONS, 10_000_000);
    for count in [MAX_ITERATIONS + 1, u32::MAX] {
        let mut client = client();
        client.first_message().unwrap();
        let server_first =
            format!("r=rOprNGfwEbeRWgbNEkqO%hv,s=W22ZaJ0SNY7soEsUEjb6gQ==,i={count}");
        assert_eq!(
            client.final_message(server_first.as_bytes()),
            Err(ScramError::Iterations(count))
        );
    }
}
