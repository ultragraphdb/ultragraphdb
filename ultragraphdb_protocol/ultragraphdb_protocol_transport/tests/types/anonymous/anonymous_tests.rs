/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::{Anonymous, ClientMechanism, Login};

#[test]
fn test_the_anonymous_login_speaks_no_mechanism() {
    let mut login = Anonymous;
    assert_eq!(login.mechanism(&[]), Ok(None));
    assert_eq!(
        login.mechanism(&["SCRAM-SHA-256".to_string(), "PLAIN".to_string()]),
        Ok(None)
    );
}

#[test]
fn test_the_anonymous_mechanism_is_empty() {
    let mut mechanism = Anonymous;
    assert_eq!(mechanism.name(), "");
    assert_eq!(mechanism.start(), Ok(Vec::new()));
    assert_eq!(mechanism.answer(b"challenge"), Ok(Vec::new()));
    assert_eq!(mechanism.finish(b"outcome"), Ok(()));
}
