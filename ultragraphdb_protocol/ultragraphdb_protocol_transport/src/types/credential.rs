/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

/// What an authentication established: the principal the connection acts for, when its credential expires in
/// microseconds of Unix time (0 for never), as the final Authenticate states it, and how long from now it is valid,
/// which the connection's deadline is set from. The authenticator, which reads the wall clock, gives both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Credential {
    principal: u64,
    expires: u64,
    valid_for: Option<Duration>,
}

impl Credential {
    /// A credential of `principal` that never expires.
    pub fn new(principal: u64) -> Self {
        Self {
            principal,
            expires: 0,
            valid_for: None,
        }
    }

    /// The credential expiring at `expires`, microseconds of Unix time, `valid_for` from now.
    pub fn expiring(principal: u64, expires: u64, valid_for: Duration) -> Self {
        Self {
            principal,
            expires,
            valid_for: Some(valid_for),
        }
    }

    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// When the credential expires, microseconds of Unix time; 0 for never.
    pub fn expires(&self) -> u64 {
        self.expires
    }

    /// How long from its exchange's end the credential is valid; `None` for never.
    pub fn valid_for(&self) -> Option<Duration> {
        self.valid_for
    }
}
