/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, Read, Write};

/// An application that answers every byte it receives with the same byte, for a [`TestPeer`](crate::TestPeer).
#[derive(Debug, Default)]
pub struct TestEcho {
    pending: Vec<u8>,
}

impl Write for TestEcho {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Read for TestEcho {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.pending.as_slice().read(buf)?;
        drop(self.pending.drain(..n));
        Ok(n)
    }
}
