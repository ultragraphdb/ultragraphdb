/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{Credentials, ScramAuthenticator, ScramVerifier};
use ultragraphdb_protocol_transport::{Authenticator, Credential, MechanismStep, ServerMechanism};

use crate::utils::rfc7677::{
    CLIENT_FINAL, CLIENT_FIRST, NAME, SECRET, SERVER_FINAL, SERVER_FIRST, SERVER_NONCE, verifier,
};

#[derive(Debug, Clone)]
struct Store;

impl Credentials for Store {
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        (name == NAME).then(|| (42, verifier()))
    }
}

fn authenticator() -> ScramAuthenticator<Store, impl FnMut() -> String> {
    ScramAuthenticator::new(Store, || SERVER_NONCE.to_string(), SECRET)
}

#[test]
fn test_an_exchange_runs_through_the_transports_traits() {
    let mut authenticator = authenticator();
    assert!(authenticator.start("PLAIN").is_none());
    let mut exchange = authenticator.start("SCRAM-SHA-256").unwrap();
    assert_eq!(
        exchange.step(CLIENT_FIRST),
        Ok(MechanismStep::Continue(SERVER_FIRST.to_vec()))
    );
    assert_eq!(
        exchange.step(CLIENT_FINAL),
        Ok(MechanismStep::Done(
            SERVER_FINAL.to_vec(),
            Credential::new(42)
        ))
    );
    assert!(exchange.step(CLIENT_FINAL).is_err(), "the exchange is over");
}

#[test]
fn test_each_exchange_takes_the_next_nonce() {
    let mut count = 0;
    let mut authenticator = ScramAuthenticator::new(
        Store,
        move || {
            count += 1;
            format!("n{count}")
        },
        SECRET,
    );
    let mut first = || match authenticator
        .start("SCRAM-SHA-256")
        .unwrap()
        .step(CLIENT_FIRST)
    {
        Ok(MechanismStep::Continue(message)) => String::from_utf8(message).unwrap(),
        other => panic!("{other:?}"),
    };
    let one = first();
    let two = first();
    assert!(one.starts_with("r=rOprNGfwEbeRWgbNEkqOn1,"), "{one}");
    assert!(two.starts_with("r=rOprNGfwEbeRWgbNEkqOn2,"), "{two}");
}
