/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{
    DEFAULT_ITERATIONS, ScramClient, ScramError, ScramServer, ScramVerifier,
};

use ultragraphdb_tool_bdd::TestResult;

use crate::utils::rfc7677::{
    CLIENT_FINAL, CLIENT_FIRST, CLIENT_NONCE, NAME, SECRET, SERVER_FINAL, SERVER_FIRST,
    SERVER_NONCE, verifier,
};

/// A lookup holding RFC 7677's user, its verifier `known`, as principal 42.
fn lookup(known: &ScramVerifier) -> impl Fn(&str) -> Option<(u64, ScramVerifier)> + Copy + '_ {
    move |name| (name == NAME).then(|| (42, known.clone()))
}

/// The server, its unknown names' verifiers at RFC 7677's count, as its known one's.
fn server() -> ScramServer {
    ScramServer::new(SERVER_NONCE, SECRET).with_iterations(4_096)
}

#[test]
fn test_the_server_answers_rfc_7677s_messages() -> TestResult {
    let known = verifier()?;
    let mut server = server();
    assert_eq!(server.first(CLIENT_FIRST, lookup(&known))?, SERVER_FIRST);
    assert_eq!(server.last(CLIENT_FINAL)?, (SERVER_FINAL.to_vec(), 42));
    Ok(())
}

/// A client's exchange with `password` against the server, to its final message.
fn exchange(name: &str, password: &str) -> TestResult<Result<(Vec<u8>, u64), ScramError>> {
    let known = verifier()?;
    let mut client = ScramClient::new(name, password, CLIENT_NONCE)?;
    let mut server = server();
    let first = match server.first(&client.first_message()?, lookup(&known)) {
        Ok(first) => first,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let last = client.final_message(&first)?;
    Ok(server.last(&last))
}

#[test]
fn test_a_wrong_password_and_an_unknown_name_fail_alike_at_the_proof() -> TestResult {
    assert_eq!(exchange(NAME, "wrong")?, Err(ScramError::proof()));
    assert_eq!(exchange("nobody", "pencil")?, Err(ScramError::proof()));
    Ok(())
}

#[test]
fn test_an_unknown_name_takes_a_verifier_from_the_servers_secret() -> TestResult {
    let first = |name: &str, secret: [u8; 32]| -> TestResult<Vec<u8>> {
        let mut client = ScramClient::new(name, "pw", CLIENT_NONCE)?;
        Ok(ScramServer::new(SERVER_NONCE, secret).first(&client.first_message()?, |_| None)?)
    };
    // The same name, the same salt; another name, another; the default count, as a new verifier takes.
    assert_eq!(first("nobody", SECRET)?, first("nobody", SECRET)?);
    assert_ne!(first("nobody", SECRET)?, first("someone", SECRET)?);
    assert_ne!(first("nobody", SECRET)?, first("nobody", [8; 32])?);
    let text = String::from_utf8(first("nobody", SECRET)?)?;
    assert!(
        text.ends_with(&format!(",i={DEFAULT_ITERATIONS}")),
        "{text}"
    );
    Ok(())
}

#[test]
fn test_the_clients_first_message_is_checked() -> TestResult {
    let known = verifier()?;
    let first = |message: &[u8]| server().first(message, lookup(&known));
    assert_eq!(
        first(b"p=tls-exporter,,n=user,r=abc"),
        Err(ScramError::channel_binding())
    );
    // A client that supports channel binding and believes the server does not: taken (RFC 5802 section 6).
    assert!(first(b"y,,n=user,r=abc").is_ok());
    assert_eq!(
        first(b"n,a=admin,n=user,r=abc"),
        Err(ScramError::malformed("authorization identity"))
    );
    assert_eq!(first(b"n,,r=abc"), Err(ScramError::malformed("name")));
    assert_eq!(first(b"n,,n=user"), Err(ScramError::malformed("nonce")));
    assert_eq!(
        first(b"n,,n=us=2Xer,r=abc"),
        Err(ScramError::malformed("name"))
    );
    assert_eq!(
        first(b"garbage"),
        Err(ScramError::malformed("channel binding"))
    );
    // A name SASLprep refuses.
    assert_eq!(
        first("n,,n=us\u{0007}er,r=abc".as_bytes()),
        Err(ScramError::prepare())
    );
    Ok(())
}

#[test]
fn test_the_clients_final_message_is_checked() -> TestResult {
    let known = verifier()?;
    let last = |message: &[u8]| -> TestResult<Result<(Vec<u8>, u64), ScramError>> {
        let mut server = server();
        assert_eq!(server.first(CLIENT_FIRST, lookup(&known))?, SERVER_FIRST);
        Ok(server.last(message))
    };
    // Another nonce than the server's answer named.
    assert_eq!(
        last(b"c=biws,r=rOprNGfwEbeRWgbNEkqOother,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=")?,
        Err(ScramError::nonce())
    );
    // Another channel binding than the first message's header.
    assert_eq!(
        last(b"c=eSws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=")?,
        Err(ScramError::channel_binding())
    );
    assert_eq!(
        last(b"c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,p=!!")?,
        Err(ScramError::malformed("proof"))
    );
    assert_eq!(
        last(b"c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0")?,
        Err(ScramError::malformed("proof"))
    );
    Ok(())
}

#[test]
fn test_messages_out_of_step_are_refused() -> TestResult {
    let known = verifier()?;
    let mut server = server();
    assert_eq!(server.last(CLIENT_FINAL), Err(ScramError::out_of_step()));
    assert_eq!(server.first(CLIENT_FIRST, lookup(&known))?, SERVER_FIRST);
    assert_eq!(
        server.first(CLIENT_FIRST, lookup(&known)),
        Err(ScramError::out_of_step())
    );
    assert_eq!(server.last(CLIENT_FINAL)?, (SERVER_FINAL.to_vec(), 42));
    assert_eq!(server.last(CLIENT_FINAL), Err(ScramError::out_of_step()));
    Ok(())
}

#[test]
fn test_a_name_carried_escaped_is_looked_up_unescaped() -> TestResult {
    // RFC 5802 section 5.1: a name's `,` travels as `=2C` and its `=` as `=3D`; the server reads them back, so the
    // name looked up is `a,b=c`.
    let mut looked_up = String::new();
    let answer = server().first(b"n,,n=a=2Cb=3Dc,r=abc", |name| {
        looked_up = name.to_string();
        None
    });
    assert!(answer.is_ok());
    assert_eq!(looked_up, "a,b=c");
    Ok(())
}

#[test]
fn test_a_channel_binding_flag_other_than_n_y_or_p_is_malformed() -> TestResult {
    // RFC 5802 section 7: gs2-cbind-flag is `p=<name>`, `n` or `y`; any other is a malformed header, not a request for
    // channel binding.
    let known = verifier()?;
    assert_eq!(
        server().first(b"x,,n=user,r=abc", lookup(&known)),
        Err(ScramError::malformed("channel binding"))
    );
    Ok(())
}

/// The name a client-first-message asks for, read before the exchange runs: RFC 7677's `user`, a name's `=2C` and `=3D`
/// unescaped (RFC 5802 §5.1), and the message refused as `first` refuses it.
#[test]
fn test_the_name_a_client_first_message_asks_for_is_read_ahead() -> TestResult {
    assert_eq!(ScramServer::name_in(CLIENT_FIRST)?, NAME);
    assert_eq!(ScramServer::name_in(b"n,,n=a=2Cb=3Dc,r=nonce")?, "a,b=c");
    assert_eq!(
        ScramServer::name_in(b"p=tls-server-end-point,,n=user,r=nonce"),
        Err(ScramError::channel_binding())
    );
    assert_eq!(
        ScramServer::name_in(b"n,,r=nonce"),
        Err(ScramError::malformed("name"))
    );
    Ok(())
}
