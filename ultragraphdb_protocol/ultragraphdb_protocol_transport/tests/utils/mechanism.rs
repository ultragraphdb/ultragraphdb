/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A test mechanism, `TEST`, shaped as SCRAM's exchange: the client names itself, the server challenges, the client
//! answers with its password, and the server welcomes it; SCRAM-SHA-256 itself is `ultragraphdb_protocol_scram`'s.

use std::time::Duration;

use ultragraphdb_protocol_transport::{
    Authenticator, ClientMechanism, Credential, MechanismStep, ServerMechanism,
};

pub const TEST: &str = "TEST";

/// When an expiring credential expires, microseconds of Unix time, as the authenticator states it on the wire.
pub const EXPIRES: u64 = 1_798_761_600_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestClient {
    name: String,
    password: String,
}

impl TestClient {
    pub fn new(name: &str, password: &str) -> Self {
        Self {
            name: name.to_string(),
            password: password.to_string(),
        }
    }
}

impl TestClient {
    /// The mechanism's name, through the trait.
    pub fn name_of(&self) -> &str {
        ClientMechanism::name(self)
    }
}

impl ClientMechanism for TestClient {
    type Error = String;

    fn name(&self) -> &str {
        TEST
    }

    fn start(&mut self) -> Result<Vec<u8>, String> {
        Ok(format!("user={}", self.name).into_bytes())
    }

    fn answer(&mut self, challenge: &[u8]) -> Result<Vec<u8>, String> {
        if challenge != b"challenge" {
            return Err("an unexpected challenge".to_string());
        }
        Ok(format!("pass={}", self.password).into_bytes())
    }

    fn finish(&mut self, outcome: &[u8]) -> Result<(), String> {
        if outcome != b"welcome" {
            return Err("the server did not prove itself".to_string());
        }
        Ok(())
    }
}

/// A user the test authenticator knows: its name, password, principal, and how long its credential lasts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub name: &'static str,
    pub password: &'static str,
    pub principal: u64,
    pub valid_for: Option<Duration>,
}

#[derive(Debug, Clone, Default)]
pub struct TestAuthenticator {
    pub users: Vec<User>,
}

impl TestAuthenticator {
    pub fn new(users: Vec<User>) -> Self {
        Self { users }
    }
}

impl Authenticator for TestAuthenticator {
    type Mechanism = TestExchange;

    fn start(&mut self, mechanism: &str) -> Option<TestExchange> {
        (mechanism == TEST).then(|| TestExchange {
            users: self.users.clone(),
            user: None,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TestExchange {
    users: Vec<User>,
    user: Option<String>,
}

impl ServerMechanism for TestExchange {
    type Error = String;

    fn step(&mut self, message: &[u8]) -> Result<MechanismStep, String> {
        let text = std::str::from_utf8(message).map_err(|e| e.to_string())?;
        match (&self.user, text.split_once('=')) {
            (None, Some(("user", name))) => {
                self.user = Some(name.to_string());
                Ok(MechanismStep::Continue(b"challenge".to_vec()))
            }
            (Some(name), Some(("pass", password))) => {
                let user = self
                    .users
                    .iter()
                    .find(|u| u.name == name && u.password == password)
                    .ok_or_else(|| "wrong password".to_string())?;
                let credential = match user.valid_for {
                    Some(valid_for) => Credential::expiring(user.principal, EXPIRES, valid_for),
                    None => Credential::new(user.principal),
                };
                Ok(MechanismStep::Done(b"welcome".to_vec(), credential))
            }
            _ => Err("out of step".to_string()),
        }
    }
}
