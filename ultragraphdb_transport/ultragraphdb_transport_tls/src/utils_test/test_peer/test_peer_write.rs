/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, Read, Write};

use super::TestPeer;

impl<A: Read + Write> Write for TestPeer<A> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.receive(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
