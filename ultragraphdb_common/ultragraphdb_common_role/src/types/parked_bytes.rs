/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::{Cell, RefCell};
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};

/// The bytes of frames a connection's session holds past their [`crate::Session::answer`], and the bound at which the connection
/// stops reading (`PROTOCOL.md` §11.12, *Parked requests* and *Frames on their way*). The holder counts, the connection reads the count at each
/// frame, and whichever lets go wakes the connection. Both sides run on one core's thread, so an `Rc` of cells holds it and no
/// lock or atomic stands on the path, and no borrow crosses a wait.
#[derive(Debug, Clone)]
pub struct ParkedBytes {
    shared: Rc<Shared>,
}

#[derive(Debug)]
struct Shared {
    bytes: Cell<usize>,
    bound: usize,
    reader: RefCell<Option<Waker>>,
}

impl ParkedBytes {
    /// A count of nothing, stopping a connection's reads at `bound` bytes.
    #[must_use]
    pub fn new(bound: usize) -> Self {
        Self {
            shared: Rc::new(Shared {
                bytes: Cell::new(0),
                bound,
                reader: RefCell::new(None),
            }),
        }
    }

    /// Counts `bytes` more.
    pub fn hold(&self, bytes: usize) {
        self.shared
            .bytes
            .set(self.shared.bytes.get().saturating_add(bytes));
    }

    /// Counts `bytes` fewer, and wakes a connection waiting for room.
    pub fn release(&self, bytes: usize) {
        self.shared
            .bytes
            .set(self.shared.bytes.get().saturating_sub(bytes));
        let reader = self.shared.reader.borrow_mut().take();
        if let Some(reader) = reader {
            reader.wake();
        }
    }

    /// The bytes held now.
    #[must_use]
    pub fn count(&self) -> usize {
        self.shared.bytes.get()
    }

    /// Waits until the bytes held stand below the bound. One connection waits at a time. A dropped future leaves its waker
    /// registered, and the next release wakes a task no longer waiting, which costs that task one poll.
    pub async fn room(&self) {
        poll_fn(|cx| {
            if self.count() < self.shared.bound {
                return Poll::Ready(());
            }
            *self.shared.reader.borrow_mut() = Some(cx.waker().clone());
            Poll::Pending
        })
        .await;
    }
}
