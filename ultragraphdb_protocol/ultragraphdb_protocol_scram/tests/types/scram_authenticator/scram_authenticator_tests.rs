/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_scram::{Credentials, ScramAuthenticator, ScramVerifier};
use ultragraphdb_protocol_transport::{Authenticator, Credential, MechanismStep, ServerMechanism};

use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::rfc7677::{
    CLIENT_FINAL, CLIENT_FIRST, NAME, SECRET, SERVER_FINAL, SERVER_FIRST, SERVER_NONCE, verifier,
};

/// A store holding RFC 7677's user as principal 42.
#[derive(Debug, Clone)]
struct Store(ScramVerifier);

impl Credentials for Store {
    fn lookup(&self, name: &str) -> Option<(u64, ScramVerifier)> {
        (name == NAME).then(|| (42, self.0.clone()))
    }
}

fn store() -> TestResult<Store> {
    Ok(Store(verifier()?))
}

fn authenticator() -> TestResult<ScramAuthenticator<Store, impl FnMut() -> String>> {
    Ok(ScramAuthenticator::new(
        store()?,
        || SERVER_NONCE.to_string(),
        SECRET,
    ))
}

#[test]
fn test_an_exchange_runs_through_the_transports_traits() -> TestResult {
    let mut authenticator = authenticator()?;
    assert!(authenticator.start("PLAIN").is_none());
    let mut exchange = authenticator
        .start("SCRAM-SHA-256")
        .context("an exchange of SCRAM-SHA-256")?;
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
    Ok(())
}

#[test]
fn test_each_exchange_takes_the_next_nonce() -> TestResult {
    let mut count = 0;
    let mut authenticator = ScramAuthenticator::new(
        store()?,
        move || {
            count += 1;
            format!("n{count}")
        },
        SECRET,
    );
    let mut first = || -> TestResult<String> {
        let step = authenticator
            .start("SCRAM-SHA-256")
            .context("an exchange of SCRAM-SHA-256")?
            .step(CLIENT_FIRST);
        let message = if let Ok(MechanismStep::Continue(message)) = &step {
            Some(message.clone())
        } else {
            None
        };
        Ok(String::from_utf8(
            message
                .ok_or_else(|| format!("{step:?}"))
                .context("the client's first message continues the exchange")?,
        )?)
    };
    let one = first()?;
    let two = first()?;
    assert!(one.starts_with("r=rOprNGfwEbeRWgbNEkqOn1,"), "{one}");
    assert!(two.starts_with("r=rOprNGfwEbeRWgbNEkqOn2,"), "{two}");
    Ok(())
}
