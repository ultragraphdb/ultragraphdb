/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::future::poll_fn;
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::task::{Poll, Waker};

use crate::types::connection_slot::ConnectionSlot;

/// The connections a listener serves at once, counted against the most it may: its accept loop takes a slot before each
/// accept, waiting while every slot is held, and each connection frees its slot as its task ends.
///
/// Shared through an `Rc` by the accept loop and its connections' tasks, all on the listener's one runtime thread, and
/// counted in `Cell`s, since no use of a field spans an `.await` (Ultra rule 9).
pub(crate) struct ConnectionSlots {
    max: NonZeroUsize,
    held: Cell<usize>,
    /// The accept loop's waker while it waits for a slot: one loop takes the slots, so one waker is all that waits.
    waiting: Cell<Option<Waker>>,
}

impl ConnectionSlots {
    /// Slots for at most `max` connections at once, none held.
    #[must_use]
    pub(crate) fn new(max: NonZeroUsize) -> Rc<Self> {
        Rc::new(Self {
            max,
            held: Cell::new(0),
            waiting: Cell::new(None),
        })
    }

    /// A slot, once fewer than the most are held, which its connection holds until it ends. A future dropped while it
    /// waits takes no slot and leaves its waker, which wakes nothing.
    pub(crate) async fn take(self: &Rc<Self>) -> ConnectionSlot {
        poll_fn(|context| {
            if self.held.get() < self.max.get() {
                Poll::Ready(())
            } else {
                self.waiting.set(Some(context.waker().clone()));
                Poll::Pending
            }
        })
        .await;
        // `held` is below `max` here, so one more stays within `usize`.
        self.held.set(self.held.get() + 1);
        ConnectionSlot::new(Rc::clone(self))
    }

    /// A slot given back by a connection that ended, and the accept loop woken if it waits for one.
    pub(crate) fn free(&self) {
        self.held.set(self.held.get().saturating_sub(1));
        if let Some(waker) = self.waiting.take() {
            waker.wake();
        }
    }
}
