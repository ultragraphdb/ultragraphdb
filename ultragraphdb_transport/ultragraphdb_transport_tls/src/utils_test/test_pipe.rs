/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::rc::Rc;

/// The bytes in flight each way between the two ends of a [`TestPipe`]: `[0]` towards the first end, `[1]` towards
/// the second.
type Lanes = [VecDeque<u8>; 2];

/// One end of an in-memory pipe, read and written as a non-blocking socket is: a write goes whole to the other end, a
/// read takes what the other end wrote, and finding nothing is `WouldBlock`. Two ends on one thread carry a TLS
/// conversation without a socket.
#[derive(Debug)]
pub struct TestPipe {
    // Shared, not copied: both ends of one pipe on one thread write into and read from the same lanes, and a
    // `RefCell` lends them to one end's call at a time.
    lanes: Rc<RefCell<Lanes>>,
    // The lane this end reads; it writes the other.
    lane: usize,
}

impl TestPipe {
    /// The two ends of a new pipe.
    #[must_use]
    pub fn pair() -> (Self, Self) {
        let lanes = Rc::new(RefCell::new([VecDeque::new(), VecDeque::new()]));
        (
            Self {
                lanes: Rc::clone(&lanes),
                lane: 0,
            },
            Self { lanes, lane: 1 },
        )
    }
}

impl Read for TestPipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut lanes = self.lanes.borrow_mut();
        let Some(incoming) = lanes.get_mut(self.lane) else {
            return Err(io::Error::other("a pipe has two lanes"));
        };
        if incoming.is_empty() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = buf.len().min(incoming.len());
        for (slot, byte) in buf.iter_mut().zip(incoming.drain(..n)) {
            *slot = byte;
        }
        Ok(n)
    }
}

impl Write for TestPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut lanes = self.lanes.borrow_mut();
        let Some(outgoing) = lanes.get_mut(1 - self.lane) else {
            return Err(io::Error::other("a pipe has two lanes"));
        };
        outgoing.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
