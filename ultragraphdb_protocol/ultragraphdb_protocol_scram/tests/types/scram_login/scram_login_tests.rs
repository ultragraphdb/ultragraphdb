/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;

use ultragraphdb_protocol_scram::{MECHANISM, ScramClient, ScramError, ScramErrorEnum, ScramLogin};
use ultragraphdb_protocol_transport::{ClientMechanism, Login, TransportErrorEnum};

fn offered() -> Vec<String> {
    vec!["PLAIN".to_string(), MECHANISM.to_string()]
}

#[test]
fn test_each_exchange_is_a_fresh_client_with_the_next_nonce() {
    let drawn = Cell::new(0);
    let mut login = ScramLogin::new("user", "pencil", || {
        drawn.set(drawn.get() + 1);
        Ok(format!("nonce{}", drawn.get()))
    })
    .unwrap();
    assert_eq!(login.name(), "user");
    let mut first = login.mechanism(&offered()).unwrap().expect("SCRAM offered");
    let mut second = login.mechanism(&offered()).unwrap().expect("SCRAM offered");
    assert_eq!(first.name(), MECHANISM);
    assert_eq!(first.start().unwrap(), b"n,,n=user,r=nonce1");
    assert_eq!(second.start().unwrap(), b"n,,n=user,r=nonce2");
    assert_eq!(
        first,
        {
            let mut expected = ScramClient::new("user", "pencil", "nonce1").unwrap();
            let _ = expected.first_message();
            expected
        },
        "the client of the login's name, password and nonce"
    );
}

#[test]
fn test_a_server_not_offering_scram_gets_no_mechanism_and_draws_no_nonce() {
    let drawn = Cell::new(0);
    let mut login = ScramLogin::new("user", "pencil", || {
        drawn.set(drawn.get() + 1);
        Ok("nonce".to_string())
    })
    .unwrap();
    assert_eq!(login.mechanism(&["PLAIN".to_string()]), Ok(None));
    assert_eq!(login.mechanism(&[]), Ok(None));
    assert_eq!(drawn.get(), 0);
}

#[test]
fn test_a_name_or_password_saslprep_refuses_is_refused_at_once() {
    let nonce = || Ok("nonce".to_string());
    for (name, password) in [("user\u{0007}", "pencil"), ("user", "pen\u{0007}cil")] {
        assert_eq!(
            ScramLogin::new(name, password, nonce).err().map(|e| e.0),
            Some(ScramErrorEnum::Prepare)
        );
    }
}

#[test]
fn test_a_nonce_the_source_fails_or_draws_malformed_is_an_authentication_error() {
    let mut failing =
        ScramLogin::new("user", "pencil", || Err(ScramError::Random("none"))).unwrap();
    let error = failing.mechanism(&offered()).unwrap_err();
    assert!(
        matches!(&error.0, TransportErrorEnum::Authentication { reason } if reason.contains("none")),
        "{error:?}"
    );
    let mut malformed = ScramLogin::new("user", "pencil", || Ok("a,b".to_string())).unwrap();
    assert!(matches!(
        malformed.mechanism(&offered()).unwrap_err().0,
        TransportErrorEnum::Authentication { .. }
    ));
}
