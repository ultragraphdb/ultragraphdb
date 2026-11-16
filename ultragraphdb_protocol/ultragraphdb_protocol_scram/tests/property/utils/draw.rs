/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The inputs the laws draw (`notes/laws.md`, group 1b): a name, a password, a salt of 1 to 64 bytes, an iteration
//! count kept at the floor so a derivation costs little, two nonces; and one message changed in one drawn place.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_protocol_scram::{
    MIN_ITERATIONS, ScramClient, ScramError, ScramServer, ScramVerifier,
};

/// The principal a drawn name is held as.
pub const PRINCIPAL: u64 = 7;

/// The secret an unknown name's verifier derives from.
pub const SECRET: [u8; 32] = [7; 32];

/// A name or a password of printable ASCII, `,` and `=` among it, which SASLprep leaves as it is; empty included.
pub fn text(tc: &TestCase, size_max: usize) -> String {
    tc.draw(
        gs::text()
            .min_codepoint(0x20)
            .max_codepoint(0x7E)
            .max_size(size_max),
    )
}

/// A nonce: printable ASCII without a comma, 1 to 24 characters (RFC 5802 §7).
pub fn nonce(tc: &TestCase) -> String {
    tc.draw(
        gs::text()
            .min_codepoint(0x21)
            .max_codepoint(0x7E)
            .exclude_characters(",")
            .min_size(1)
            .max_size(24),
    )
}

/// A salt of 1 to 64 bytes.
pub fn salt(tc: &TestCase) -> Vec<u8> {
    tc.draw(gs::vecs(gs::integers::<u8>()).min_size(1).max_size(64))
}

/// A count a client runs, at the floor: 4,096 to 4,103.
pub fn iterations(tc: &TestCase) -> u32 {
    tc.draw(
        gs::integers::<u32>()
            .min_value(MIN_ITERATIONS)
            .max_value(MIN_ITERATIONS + 7),
    )
}

/// `message` changed in one drawn place: a byte replaced by another, the message cut short, or a byte added.
pub fn alter(tc: &TestCase, message: &[u8]) -> Vec<u8> {
    let mut altered = message.to_vec();
    let at = tc.draw(gs::integers::<usize>().max_value(message.len()));
    let byte = tc.draw(gs::integers::<u8>());
    match (
        tc.draw(gs::integers::<u8>().max_value(2)),
        altered.get_mut(at),
    ) {
        (0, Some(old)) => *old = if *old == byte { byte ^ 1 } else { byte },
        (1, _) => altered.truncate(at),
        _ => altered.insert(at, byte),
    }
    altered
}

/// A drawn exchange: a client's name, password and nonce, the server's nonce, and the verifier the name is held with,
/// derived from `held`, the client's password or another.
pub struct Drawn {
    pub name: String,
    pub password: String,
    pub client_nonce: String,
    pub server_nonce: String,
    pub verifier: ScramVerifier,
}

/// An exchange of drawn inputs, its verifier derived from the client's password or, when `wrong`, from another.
pub fn exchange(tc: &TestCase, wrong: bool) -> Result<Drawn, ScramError> {
    let name = text(tc, 12);
    let password = text(tc, 16);
    let held = if wrong {
        format!("{password}!")
    } else {
        password.clone()
    };
    Ok(Drawn {
        name,
        password,
        client_nonce: nonce(tc),
        server_nonce: nonce(tc),
        verifier: ScramVerifier::derive(&held, &salt(tc), iterations(tc))?,
    })
}

impl Drawn {
    /// The group's principals: the drawn name, as [`PRINCIPAL`], and no other.
    pub fn lookup(&self) -> impl Fn(&str) -> Option<(u64, ScramVerifier)> + Copy + '_ {
        move |name| (name == self.name).then(|| (PRINCIPAL, self.verifier.clone()))
    }

    /// The drawn client.
    pub fn client(&self) -> Result<ScramClient, ScramError> {
        ScramClient::new(&self.name, &self.password, &self.client_nonce)
    }

    /// `ScramServer` with the drawn server nonce, its unknown names' verifiers at the held verifier's count.
    pub fn server(&self) -> ScramServer {
        ScramServer::new(&self.server_nonce, SECRET).with_iterations(self.verifier.iterations())
    }
}
