/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, Read};

use super::Connection;

impl Read for Connection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(socket) => socket.read(buf),
            Self::Tls(tls) => tls.read(buf),
        }
    }
}
