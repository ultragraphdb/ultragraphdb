/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;

use ultragraphdb_protocol_scram::{MECHANISM, ScramClient, ScramError, ScramErrorEnum, ScramLogin};
use ultragraphdb_protocol_transport::{ClientMechanism, Login, TransportErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

fn offered() -> Vec<String> {
    vec!["PLAIN".to_string(), MECHANISM.to_string()]
}

#[test]
fn test_each_exchange_is_a_fresh_client_with_the_next_nonce() -> TestResult {
    let drawn = Cell::new(0);
    let mut login = ScramLogin::new("user", "pencil", || {
        drawn.set(drawn.get() + 1);
        Ok(format!("nonce{}", drawn.get()))
    })?;
    assert_eq!(login.name(), "user");
    let mut first = login.mechanism(&offered())?.context("SCRAM offered")?;
    let mut second = login.mechanism(&offered())?.context("SCRAM offered")?;
    assert_eq!(first.name(), MECHANISM);
    assert_eq!(first.start()?, b"n,,n=user,r=nonce1");
    assert_eq!(second.start()?, b"n,,n=user,r=nonce2");
    assert_eq!(
        first,
        {
            let mut expected = ScramClient::new("user", "pencil", "nonce1")?;
            assert_eq!(expected.first_message()?, b"n,,n=user,r=nonce1");
            expected
        },
        "the client of the login's name, password and nonce"
    );
    Ok(())
}

#[test]
fn test_a_server_not_offering_scram_gets_no_mechanism_and_draws_no_nonce() -> TestResult {
    let drawn = Cell::new(0);
    let mut login = ScramLogin::new("user", "pencil", || {
        drawn.set(drawn.get() + 1);
        Ok("nonce".to_string())
    })?;
    assert_eq!(login.mechanism(&["PLAIN".to_string()]), Ok(None));
    assert_eq!(login.mechanism(&[]), Ok(None));
    assert_eq!(drawn.get(), 0);
    Ok(())
}

#[test]
fn test_a_name_or_password_saslprep_refuses_is_refused_at_once() -> TestResult {
    let nonce = || Ok("nonce".to_string());
    for (name, password) in [("user\u{0007}", "pencil"), ("user", "pen\u{0007}cil")] {
        assert_eq!(
            ScramLogin::new(name, password, nonce).err().map(|e| e.0),
            Some(ScramErrorEnum::Prepare)
        );
    }
    Ok(())
}

#[test]
fn test_a_nonce_the_source_fails_or_draws_malformed_is_an_authentication_error() -> TestResult {
    let mut failing = ScramLogin::new("user", "pencil", || Err(ScramError::random("none")))?;
    let Err(error) = failing.mechanism(&offered()) else {
        return Err(TestError::new(
            "a failing nonce source refuses the mechanism",
        ));
    };
    assert!(
        matches!(&error.0, TransportErrorEnum::Authentication { reason } if reason.contains("none")),
        "{error:?}"
    );
    let mut malformed = ScramLogin::new("user", "pencil", || Ok("a,b".to_string()))?;
    assert!(matches!(
        malformed.mechanism(&offered()).err().map(|e| e.0),
        Some(TransportErrorEnum::Authentication { .. })
    ));
    Ok(())
}
