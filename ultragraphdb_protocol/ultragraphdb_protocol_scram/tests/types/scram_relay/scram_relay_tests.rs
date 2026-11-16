/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The endpoint's half of the exchange. Each corner case of group 1b (`notes/corner-cases.md`, C1b.1 to C1b.8, C1b.13
//! and C1b.14) runs through the relay and through `ScramServer`, the oracle, and the two agree; RFC 7677's exchange is
//! the published anchor.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ultragraphdb_protocol_scram::{
    MIN_ITERATIONS, ScramClient, ScramError, ScramRelay, ScramServer, ScramVerifier,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::halves::Halves;
use crate::utils::rfc7677::{
    AUTH_MESSAGE, CLIENT_FINAL, CLIENT_FIRST, CLIENT_NONCE, CLIENT_PROOF, ITERATIONS, NAME, SALT,
    SECRET, SERVER_FINAL, SERVER_FIRST, SERVER_NONCE, SERVER_SIGNATURE, verifier,
};

/// The principal RFC 7677's user is held as.
const PRINCIPAL: u64 = 42;

/// A lookup holding RFC 7677's user, its verifier `known`, as [`PRINCIPAL`].
fn lookup(known: &ScramVerifier) -> impl Fn(&str) -> Option<(u64, ScramVerifier)> + Copy + '_ {
    move |name| (name == NAME).then(|| (PRINCIPAL, known.clone()))
}

/// The server, its unknown names' verifiers at RFC 7677's count.
fn server() -> ScramServer {
    ScramServer::new(SERVER_NONCE, SECRET).with_iterations(ITERATIONS)
}

/// The name `ScramServer` looks up for `client_first`, or its refusal.
fn server_reads(client_first: &[u8]) -> TestResult<Result<String, ScramError>> {
    let mut looked_up = None;
    if let Err(refusal) = server().first(client_first, |name| {
        looked_up = Some(name.to_string());
        None
    }) {
        return Ok(Err(refusal));
    }
    Ok(Ok(
        looked_up.context("an accepted first message names a name")?
    ))
}

/// What the relay reads of `client_first`, checked against what `ScramServer` reads and what `ScramServer::name_in`
/// reads where it reads a name.
fn relay_reads(client_first: &[u8]) -> TestResult<Result<String, ScramError>> {
    let read = ScramRelay::new(SERVER_NONCE).first(client_first);
    assert_eq!(read, server_reads(client_first)?);
    if let Ok(name) = &read {
        assert_eq!(ScramServer::name_in(client_first).as_ref(), Ok(name));
    }
    Ok(read)
}

/// The server-final-message and principal of `client_final` after RFC 7677's client-first-message, through the two
/// halves and through `ScramServer`, which agree; the halves' answer.
fn both_last(client_final: &[u8]) -> TestResult<Result<(Vec<u8>, u64), ScramError>> {
    let known = verifier()?;
    let mut halves = Halves::new(SERVER_NONCE, SECRET, ITERATIONS, lookup(&known));
    let mut server = server();
    assert_eq!(halves.first(CLIENT_FIRST)?, SERVER_FIRST);
    assert_eq!(server.first(CLIENT_FIRST, lookup(&known))?, SERVER_FIRST);
    let answer = halves.last(client_final);
    assert_eq!(answer, server.last(client_final));
    Ok(answer)
}

/// C1b.13: RFC 7677 §3's exchange, user `user`, password `pencil`, through the relay and the group's half: the
/// published name, server-first-message, ClientProof, ServerSignature and server-final-message, and RFC 5802 §3's
/// AuthMessage of the published messages.
#[test]
fn test_rfc_7677s_exchange_runs_through_both_halves() -> TestResult {
    let mut relay = ScramRelay::new(SERVER_NONCE);
    assert_eq!(relay.first(CLIENT_FIRST)?, NAME);
    assert_eq!(relay.challenge(&SALT, ITERATIONS)?, SERVER_FIRST);
    let (auth_message, proof) = relay.last(CLIENT_FINAL)?;
    assert_eq!(auth_message, AUTH_MESSAGE);
    assert_eq!(
        proof.to_vec(),
        STANDARD.decode(CLIENT_PROOF).context("RFC 7677's proof")?
    );
    let signature = verifier()?
        .sign(&auth_message, &proof)
        .context("RFC 7677's proof holds")?;
    assert_eq!(
        signature.to_vec(),
        STANDARD
            .decode(SERVER_SIGNATURE)
            .context("RFC 7677's signature")?
    );
    assert_eq!(relay.finish(&signature)?, SERVER_FINAL);
    Ok(())
}

/// C1b.1: a name of one character, and a name holding `,` and `=` sent as `=2C` and `=3D`, are read as
/// `ScramServer::name_in` reads them.
#[test]
fn test_the_relay_reads_the_name_the_server_reads() -> TestResult {
    assert_eq!(relay_reads(b"n,,n=a,r=abc")?, Ok("a".to_string()));
    // RFC 5802 §5.1: `,` travels as `=2C`, `=` as `=3D`.
    assert_eq!(
        relay_reads(b"n,,n=a=2Cb=3Dc,r=abc")?,
        Ok("a,b=c".to_string())
    );
    assert_eq!(relay_reads(CLIENT_FIRST)?, Ok(NAME.to_string()));
    Ok(())
}

/// C1b.2: a name of no character, and a name SASLprep changes, are read as the server reads them, and the group
/// answers each as a name no principal holds: the server-first-message `ScramServer` sends for it.
#[test]
fn test_an_empty_name_and_a_mapped_name_are_answered_as_names_not_held() -> TestResult {
    assert_eq!(relay_reads(b"n,,n=,r=abc")?, Ok(String::new()));
    // RFC 4013 §2.2: a soft hyphen maps to nothing.
    assert_eq!(
        relay_reads("n,,n=us\u{00AD}er,r=abc".as_bytes())?,
        Ok("user".to_string())
    );
    let no_one = |_: &str| None;
    for message in [&b"n,,n=,r=abc"[..], "n,,n=no\u{00AD}one,r=abc".as_bytes()] {
        let mut halves = Halves::new(SERVER_NONCE, SECRET, ITERATIONS, no_one);
        assert_eq!(halves.first(message)?, server().first(message, no_one)?);
    }
    Ok(())
}

/// C1b.3: an authorization identity, the `y` and the `p` channel-binding flags, a mandatory extension before the
/// name, and an extension after the nonce, read as `ScramServer` reads them.
#[test]
fn test_a_client_first_message_is_refused_as_the_server_refuses_it() -> TestResult {
    assert_eq!(
        relay_reads(b"n,a=admin,n=user,r=abc")?,
        Err(ScramError::malformed("authorization identity"))
    );
    // RFC 5802 §6: a client supporting channel binding that believes the server does not is taken.
    assert_eq!(relay_reads(b"y,,n=user,r=abc")?, Ok(NAME.to_string()));
    assert_eq!(
        relay_reads(b"p=tls-exporter,,n=user,r=abc")?,
        Err(ScramError::channel_binding())
    );
    assert_eq!(
        relay_reads(b"n,,m=ext,n=user,r=abc")?,
        Err(ScramError::malformed("name"))
    );
    // RFC 5802 §5.1: an extension the server does not know after the nonce is ignored.
    assert_eq!(relay_reads(b"n,,n=user,r=abc,x=ext")?, Ok(NAME.to_string()));
    assert_eq!(
        relay_reads(b"n,,n=user")?,
        Err(ScramError::malformed("nonce"))
    );
    // Whatever follows the nonce is not read, so a trailing part not of RFC 5802 §7's `attr-val` form is taken as
    // `ScramServer` takes it (`notes/group-1b.md`, *Left open*).
    assert_eq!(relay_reads(b"n,,n=user,r=a,c")?, Ok(NAME.to_string()));
    assert_eq!(
        relay_reads(b"x,,n=user,r=abc")?,
        Err(ScramError::malformed("channel binding"))
    );
    assert_eq!(
        relay_reads(b"n,,n=us=2Xer,r=abc")?,
        Err(ScramError::malformed("name"))
    );
    assert_eq!(
        relay_reads("n,,n=us\u{0007}er,r=abc".as_bytes())?,
        Err(ScramError::prepare())
    );
    assert_eq!(
        relay_reads(b"\xff,,n=user,r=abc")?,
        Err(ScramError::malformed("channel binding"))
    );
    Ok(())
}

/// The server-first-message `ScramServer` sends for RFC 7677's user held with `salt` and `iterations`.
fn server_first_of(salt: &[u8], iterations: u32) -> TestResult<Vec<u8>> {
    let held = ScramVerifier::new(salt.to_vec(), iterations, [1; 32], [2; 32]);
    Ok(server().first(CLIENT_FIRST, lookup(&held))?)
}

/// The relay's server-first-message for RFC 7677's client-first-message, `salt` and `iterations`.
fn relay_first_of(salt: &[u8], iterations: u32) -> TestResult<Vec<u8>> {
    let mut relay = ScramRelay::new(SERVER_NONCE);
    assert_eq!(relay.first(CLIENT_FIRST)?, NAME);
    Ok(relay.challenge(salt, iterations)?)
}

/// C1b.4: a salt of 1, of 16 and of 64 bytes handed to the relay gives the server-first-message `ScramServer` builds.
#[test]
fn test_the_server_first_message_of_a_salt_is_the_servers() -> TestResult {
    for salt in [vec![0x5b], vec![0xa5; 16], (0..64).collect::<Vec<u8>>()] {
        assert_eq!(
            relay_first_of(&salt, ITERATIONS)?,
            server_first_of(&salt, ITERATIONS)?
        );
    }
    // The distinct salts give distinct messages.
    assert_ne!(
        relay_first_of(&[0x5b], ITERATIONS)?,
        relay_first_of(&[0xa5; 16], ITERATIONS)?
    );
    Ok(())
}

/// C1b.5: the least count a client runs, and one below it: the relay builds each as `ScramServer` builds it, and a
/// client refuses the count below the least as it refuses `ScramServer`'s (`notes/group-1b.md`, *Found wanting*, 2).
#[test]
fn test_the_least_count_and_one_below_it_are_built_as_the_server_builds_them() -> TestResult {
    for iterations in [MIN_ITERATIONS, MIN_ITERATIONS - 1] {
        let built = relay_first_of(&SALT, iterations)?;
        assert_eq!(built, server_first_of(&SALT, iterations)?);
        let mut client = ScramClient::new(NAME, "pencil", CLIENT_NONCE)?;
        assert_eq!(client.first_message()?, CLIENT_FIRST);
        let answered = client.final_message(&built);
        if iterations < MIN_ITERATIONS {
            assert_eq!(answered, Err(ScramError::iterations(iterations)));
        } else {
            assert!(answered.is_ok());
        }
    }
    Ok(())
}

/// C1b.6: a client-final-message whose nonce is the client's alone, another exchange's, or one character short is
/// refused as `ScramServer` refuses it.
#[test]
fn test_a_nonce_other_than_the_exchanges_is_refused() -> TestResult {
    let proof = format!(",p={CLIENT_PROOF}");
    let full = format!("{CLIENT_NONCE}{SERVER_NONCE}");
    let short = full
        .get(..full.len() - 1)
        .context("a nonce of characters")?;
    for nonce in [
        CLIENT_NONCE.to_string(),
        format!("{CLIENT_NONCE}another"),
        short.to_string(),
    ] {
        let message = format!("c=biws,r={nonce}{proof}");
        assert_eq!(both_last(message.as_bytes())?, Err(ScramError::nonce()));
    }
    Ok(())
}

/// C1b.7: a channel binding other than `biws`, the first message's `n,,`, is refused as `ScramServer` refuses it.
#[test]
fn test_a_channel_binding_other_than_the_first_messages_is_refused() -> TestResult {
    let rest = format!("r={CLIENT_NONCE}{SERVER_NONCE},p={CLIENT_PROOF}");
    for binding in ["eSws", "abc", ""] {
        let message = format!("c={binding},{rest}");
        assert_eq!(
            both_last(message.as_bytes())?,
            Err(ScramError::channel_binding())
        );
    }
    assert_eq!(
        both_last(rest.as_bytes())?,
        Err(ScramError::malformed("channel binding"))
    );
    Ok(())
}

/// The channel binding follows the first message's flag (RFC 5802 §7): after `y,,` it is `eSws`, the base64 of `y,,`,
/// and `biws` is refused, as `ScramServer` has them; the AuthMessage carries the binding the client sent.
#[test]
fn test_the_channel_binding_follows_the_first_messages_flag() -> TestResult {
    let known = verifier()?;
    let client_first = b"y,,n=user,r=rOprNGfwEbeRWgbNEkqO";
    let answer = |client_final: &[u8]| -> TestResult<Result<(Vec<u8>, u64), ScramError>> {
        let mut halves = Halves::new(SERVER_NONCE, SECRET, ITERATIONS, lookup(&known));
        let mut server = server();
        assert_eq!(halves.first(client_first)?, SERVER_FIRST);
        assert_eq!(server.first(client_first, lookup(&known))?, SERVER_FIRST);
        let answer = halves.last(client_final);
        assert_eq!(answer, server.last(client_final));
        Ok(answer)
    };
    let rest = format!("r={CLIENT_NONCE}{SERVER_NONCE},p={CLIENT_PROOF}");
    assert_eq!(
        answer(format!("c=biws,{rest}").as_bytes())?,
        Err(ScramError::channel_binding())
    );
    // The binding holds; the RFC's proof, made over `c=biws`, does not.
    assert_eq!(
        answer(format!("c=eSws,{rest}").as_bytes())?,
        Err(ScramError::proof())
    );
    let mut relay = ScramRelay::new(SERVER_NONCE);
    assert_eq!(relay.first(client_first)?, NAME);
    assert_eq!(relay.challenge(&SALT, ITERATIONS)?, SERVER_FIRST);
    let (auth_message, _) = relay.last(format!("c=eSws,{rest}").as_bytes())?;
    assert_eq!(auth_message, AUTH_MESSAGE.replace("c=biws", "c=eSws"));
    Ok(())
}

/// C1b.8: a proof that is no base64, of 31 and of 33 bytes decoded, or missing is refused as `ScramServer` refuses
/// it.
#[test]
fn test_a_proof_not_in_its_form_is_refused() -> TestResult {
    let without_proof = format!("c=biws,r={CLIENT_NONCE}{SERVER_NONCE}");
    let malformed = format!("{without_proof},p=!!");
    assert_eq!(
        both_last(malformed.as_bytes())?,
        Err(ScramError::malformed("proof"))
    );
    assert_eq!(
        both_last(without_proof.as_bytes())?,
        Err(ScramError::malformed("proof"))
    );
    assert_eq!(both_last(b"\xff")?, Err(ScramError::malformed("proof")));
    let rfc_proof = STANDARD.decode(CLIENT_PROOF).context("RFC 7677's proof")?;
    let mut long = rfc_proof.clone();
    long.push(0);
    for proof in [rfc_proof.get(..31).context("31 bytes")?.to_vec(), long] {
        let message = format!("{without_proof},p={}", STANDARD.encode(&proof));
        assert_eq!(both_last(message.as_bytes())?, Err(ScramError::proof()));
    }
    Ok(())
}

/// The relay reads a 31-byte proof's message no further than its nonce: a wrong nonce with a short proof is refused
/// for its nonce, the earlier rule, as `ScramServer` refuses it.
#[test]
fn test_the_nonce_is_checked_before_the_proofs_length() -> TestResult {
    let short = STANDARD.encode([0u8; 31]);
    let message = format!("c=biws,r={CLIENT_NONCE},p={short}");
    assert_eq!(both_last(message.as_bytes())?, Err(ScramError::nonce()));
    Ok(())
}

/// The AuthMessage keeps an extension the client-final-message carries after its nonce, as RFC 5802 §3 has it, so the
/// RFC's proof no longer holds for it and the exchange is refused as `ScramServer` refuses it.
#[test]
fn test_an_extension_after_the_nonce_is_part_of_the_auth_message() -> TestResult {
    let message = format!("c=biws,r={CLIENT_NONCE}{SERVER_NONCE},x=ext,p={CLIENT_PROOF}");
    assert_eq!(both_last(message.as_bytes())?, Err(ScramError::proof()));
    let mut relay = ScramRelay::new(SERVER_NONCE);
    assert_eq!(relay.first(CLIENT_FIRST)?, NAME);
    assert_eq!(relay.challenge(&SALT, ITERATIONS)?, SERVER_FIRST);
    let (auth_message, _) = relay.last(message.as_bytes())?;
    assert_eq!(auth_message, format!("{AUTH_MESSAGE},x=ext"));
    Ok(())
}

/// The exchange of a client of `password` against RFC 7677's user held as the verifier of `held`, through the halves
/// and through `ScramServer`, which agree; the halves' answer, after the client checked a server-final-message.
fn exchange(password: &str, held: &str) -> TestResult<Result<(Vec<u8>, u64), ScramError>> {
    let known = ScramVerifier::derive(held, &SALT, ITERATIONS)?;
    let mut client = ScramClient::new(NAME, password, CLIENT_NONCE)?;
    let client_first = client.first_message()?;
    let mut halves = Halves::new(SERVER_NONCE, SECRET, ITERATIONS, lookup(&known));
    let mut server = server();
    let server_first = halves.first(&client_first)?;
    assert_eq!(server_first, server.first(&client_first, lookup(&known))?);
    let client_final = client.final_message(&server_first)?;
    let answer = halves.last(&client_final);
    assert_eq!(answer, server.last(&client_final));
    if let Ok((server_final, _)) = &answer {
        client.verify(server_final)?;
    }
    Ok(answer)
}

/// C1b.14: a password of no character, and a password SASLprep changes, are answered as `ScramServer` answers them:
/// each logs in against the verifier of what it prepares to, and against no other.
#[test]
fn test_an_empty_password_and_a_mapped_password_are_answered_as_the_server_answers_them()
-> TestResult {
    assert_eq!(
        exchange("", "")?.map(|(_, principal)| principal),
        Ok(PRINCIPAL)
    );
    // RFC 4013 §2.2: a soft hyphen maps to nothing.
    assert_eq!(
        exchange("pen\u{00AD}cil", "pencil")?.map(|(_, principal)| principal),
        Ok(PRINCIPAL)
    );
    assert_eq!(exchange("pencil", "")?, Err(ScramError::proof()));
    assert_eq!(exchange("", "pencil")?, Err(ScramError::proof()));
    Ok(())
}

/// A message out of step is refused, and a refused message leaves the relay where it stood. A second
/// client-final-message is read after the first, as `ScramServer` reads one after a proof it refused.
#[test]
fn test_messages_out_of_step_are_refused() -> TestResult {
    let out_of_step = Some(ScramError::out_of_step());
    let signature = [0; 32];
    let mut relay = ScramRelay::new(SERVER_NONCE);
    assert_eq!(
        relay.challenge(&SALT, ITERATIONS).err(),
        out_of_step.clone()
    );
    assert_eq!(relay.last(CLIENT_FINAL).err(), out_of_step.clone());
    assert_eq!(relay.finish(&signature).err(), out_of_step.clone());
    assert_eq!(
        relay.first(b"garbage"),
        Err(ScramError::malformed("channel binding"))
    );
    assert_eq!(relay.first(CLIENT_FIRST)?, NAME);
    assert_eq!(relay.first(CLIENT_FIRST).err(), out_of_step.clone());
    assert_eq!(relay.last(CLIENT_FINAL).err(), out_of_step.clone());
    assert_eq!(relay.finish(&signature).err(), out_of_step.clone());
    assert_eq!(relay.challenge(&SALT, ITERATIONS)?, SERVER_FIRST);
    assert_eq!(
        relay.challenge(&SALT, ITERATIONS).err(),
        out_of_step.clone()
    );
    assert_eq!(relay.finish(&signature).err(), out_of_step.clone());
    assert_eq!(relay.last(b"c=biws"), Err(ScramError::malformed("proof")));
    let read = relay.last(CLIENT_FINAL)?;
    assert_eq!(read.0, AUTH_MESSAGE);
    assert_eq!(relay.last(CLIENT_FINAL)?, read);
    let signature = verifier()?
        .sign(&read.0, &read.1)
        .context("RFC 7677's proof holds")?;
    assert_eq!(relay.finish(&signature)?, SERVER_FINAL);
    assert_eq!(relay.finish(&signature).err(), out_of_step.clone());
    assert_eq!(relay.last(CLIENT_FINAL).err(), out_of_step.clone());
    assert_eq!(
        relay.challenge(&SALT, ITERATIONS).err(),
        out_of_step.clone()
    );
    assert_eq!(relay.first(CLIENT_FIRST).err(), out_of_step);
    Ok(())
}

/// The forms a key may take in text: its bytes as `Debug` prints them, hex, and base64 with and without padding.
fn forms(key: &[u8; 32]) -> Vec<String> {
    let base64 = STANDARD.encode(key);
    vec![
        format!("{key:?}"),
        key.iter().map(|b| format!("{b:02x}")).collect(),
        base64.trim_end_matches('=').to_string(),
        base64,
    ]
}

/// The relay holds no key (client-endpoint, "The frontend never holds a stored key"): neither the messages it built
/// in RFC 7677's exchange nor its `Debug` hold the stored key or the server key, in any of their forms, while the
/// verifier's `Debug` does, which shows the check finds a key where one is.
#[test]
fn test_the_relay_holds_no_key() -> TestResult {
    let known = verifier()?;
    let mut halves = Halves::new(SERVER_NONCE, SECRET, ITERATIONS, lookup(&known));
    let server_first = halves.first(CLIENT_FIRST)?;
    let (server_final, _) = halves.last(CLIENT_FINAL)?;
    let relay = format!("{:?}", halves.relay());
    let held = format!("{known:?}");
    for key in [known.stored_key(), known.server_key()] {
        assert!(server_first.windows(32).all(|run| run != key));
        assert!(server_final.windows(32).all(|run| run != key));
        for form in forms(key) {
            assert!(!relay.contains(&form), "{form}");
            assert!(!String::from_utf8(server_first.clone())?.contains(&form));
            assert!(!String::from_utf8(server_final.clone())?.contains(&form));
        }
        assert!(held.contains(&format!("{key:?}")));
    }
    Ok(())
}
