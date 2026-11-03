/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, Read, Write};

use super::TestPeer;

impl<A: Read + Write> Read for TestPeer<A> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Ok(self.send(buf))
    }
}
