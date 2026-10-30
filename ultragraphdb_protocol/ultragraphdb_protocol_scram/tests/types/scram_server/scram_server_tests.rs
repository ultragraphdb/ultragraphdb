/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{
    DEFAULT_ITERATIONS, ScramClient, ScramError, ScramServer, ScramVerifier,
};

use crate::utils::rfc7677::{
    CLIENT_FINAL, CLIENT_FIRST, CLIENT_NONCE, NAME, SECRET, SERVER_FINAL, SERVER_FIRST,
    SERVER_NONCE, verifier,
};

fn lookup(name: &str) -> Option<(u64, ScramVerifier)> {
    (name == NAME).then(|| (42, verifier()))
}

/// The server, its unknown names' verifiers at RFC 7677's count, as its known one's.
fn server() -> ScramServer {
    ScramServer::new(SERVER_NONCE, SECRET).with_iterations(4_096)
}

#[test]
fn test_the_server_answers_rfc_7677s_messages() {
    let mut server = server();
    assert_eq!(server.first(CLIENT_FIRST, lookup).unwrap(), SERVER_FIRST);
    assert_eq!(
        server.last(CLIENT_FINAL).unwrap(),
        (SERVER_FINAL.to_vec(), 42)
    );
}

/// A client's exchange with `password` against the server, to its final message.
fn exchange(name: &str, password: &str) -> Result<(Vec<u8>, u64), ScramError> {
    let mut client = ScramClient::new(name, password, CLIENT_NONCE).unwrap();
    let mut server = server();
    let first = server.first(&client.first_message().unwrap(), lookup)?;
    let last = client.final_message(&first).unwrap();
    server.last(&last)
}

#[test]
fn test_a_wrong_password_and_an_unknown_name_fail_alike_at_the_proof() {
    assert_eq!(exchange(NAME, "wrong"), Err(ScramError::Proof()));
    assert_eq!(exchange("nobody", "pencil"), Err(ScramError::Proof()));
}

#[test]
fn test_an_unknown_name_takes_a_verifier_from_the_servers_secret() {
    let first = |name: &str, secret: [u8; 32]| {
        let mut client = ScramClient::new(name, "pw", CLIENT_NONCE).unwrap();
        ScramServer::new(SERVER_NONCE, secret)
            .first(&client.first_message().unwrap(), |_| None)
            .unwrap()
    };
    // The same name, the same salt; another name, another; the default count, as a new verifier takes.
    assert_eq!(first("nobody", SECRET), first("nobody", SECRET));
    assert_ne!(first("nobody", SECRET), first("someone", SECRET));
    assert_ne!(first("nobody", SECRET), first("nobody", [8; 32]));
    let text = String::from_utf8(first("nobody", SECRET)).unwrap();
    assert!(
        text.ends_with(&format!(",i={DEFAULT_ITERATIONS}")),
        "{text}"
    );
}

#[test]
fn test_the_clients_first_message_is_checked() {
    let first = |message: &[u8]| server().first(message, lookup);
    assert_eq!(
        first(b"p=tls-exporter,,n=user,r=abc"),
        Err(ScramError::ChannelBinding())
    );
    // A client that supports channel binding and believes the server does not: taken (RFC 5802 section 6).
    assert!(first(b"y,,n=user,r=abc").is_ok());
    assert_eq!(
        first(b"n,a=admin,n=user,r=abc"),
        Err(ScramError::Malformed("authorization identity"))
    );
    assert_eq!(first(b"n,,r=abc"), Err(ScramError::Malformed("name")));
    assert_eq!(first(b"n,,n=user"), Err(ScramError::Malformed("nonce")));
    assert_eq!(
        first(b"n,,n=us=2Xer,r=abc"),
        Err(ScramError::Malformed("name"))
    );
    assert_eq!(
        first(b"garbage"),
        Err(ScramError::Malformed("channel binding"))
    );
    // A name SASLprep refuses.
    assert_eq!(
        first("n,,n=us\u{0007}er,r=abc".as_bytes()),
        Err(ScramError::Prepare())
    );
}

#[test]
fn test_the_clients_final_message_is_checked() {
    let last = |message: &[u8]| {
        let mut server = server();
        server.first(CLIENT_FIRST, lookup).unwrap();
        server.last(message)
    };
    // Another nonce than the server's answer named.
    assert_eq!(
        last(b"c=biws,r=rOprNGfwEbeRWgbNEkqOother,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="),
        Err(ScramError::Nonce())
    );
    // Another channel binding than the first message's header.
    assert_eq!(
        last(b"c=eSws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ="),
        Err(ScramError::ChannelBinding())
    );
    assert_eq!(
        last(b"c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=!!"),
        Err(ScramError::Malformed("proof"))
    );
    assert_eq!(
        last(b"c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0"),
        Err(ScramError::Malformed("proof"))
    );
}

#[test]
fn test_messages_out_of_step_are_refused() {
    let mut server = server();
    assert_eq!(server.last(CLIENT_FINAL), Err(ScramError::OutOfStep()));
    server.first(CLIENT_FIRST, lookup).unwrap();
    assert_eq!(
        server.first(CLIENT_FIRST, lookup),
        Err(ScramError::OutOfStep())
    );
    server.last(CLIENT_FINAL).unwrap();
    assert_eq!(server.last(CLIENT_FINAL), Err(ScramError::OutOfStep()));
}
