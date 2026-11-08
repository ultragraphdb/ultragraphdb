/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use compio::runtime::{JoinHandle, spawn};

use crate::types::link::Link;
use crate::types::pin_target::PinTarget;
use crate::types::read_times::ReadTimes;
use crate::types::scan_stream::ScanStream;
use crate::utils::clock::now;

/// The versions a read has chosen, one per partition: what keeps them readable across a read that outlasts `read_retain_ms`
/// (`PROTOCOL.md` §11.9, quad-reads "A partition retains what a read may still read"). Dropped with its read, which drops the task
/// keeping each and so cancels each Scan. By default those of a read-write transaction, whose Bind holds its version, which adds none.
#[derive(Debug, Default)]
pub(crate) struct ReadPins {
    /// When a version chosen begins to be held; `None` for at once.
    from: Option<Instant>,
    every: Duration,
    /// The frames the read has taken, shared by an `Rc` with each pin's task on the read's one core, which compares it
    /// across its grants to see whether the read goes on.
    taken: Rc<Cell<u64>>,
    /// What keeps each partition's version, named by the partition and the version.
    keeping: RefCell<Vec<(u32, u64, JoinHandle<()>)>>,
}

impl ReadPins {
    /// The pins of a read whose version was chosen before it began: each held from the frame that first says it, so a next Scan of
    /// a read that starts late in the retention finds it held.
    pub(crate) fn at_once(times: ReadTimes) -> Self {
        Self {
            from: None,
            every: times.pin_every(),
            taken: Rc::default(),
            keeping: RefCell::default(),
        }
    }

    /// The pins of a read that chooses its version as it begins: at that version's newest, it stays in the retention for as long
    /// as a short read lasts, which then opens no Scan for it.
    pub(crate) fn delayed(times: ReadTimes) -> Self {
        Self {
            from: now().checked_add(times.pin_after()),
            ..Self::at_once(times)
        }
    }

    /// `version` of the partition of `link` is what the read reads there. It is held already when the read chose it before.
    pub(crate) fn add(&self, link: &Rc<Link>, tenant: u64, principal: u64, version: u64) {
        let partition = link.partition();
        let mut keeping = self.keeping.borrow_mut();
        if keeping
            .iter()
            .any(|(held, chosen, _)| *held == partition && *chosen == version)
        {
            return;
        }
        let target = PinTarget {
            link: Rc::clone(link),
            tenant,
            principal,
            version,
        };
        // Sent before anything that follows on the link, so a Scan after this frame finds the version held or says what became of it.
        let opened = self.from.is_none().then(|| target.open());
        let wait = self
            .from
            .map_or(Duration::ZERO, |from| from.saturating_duration_since(now()));
        let kept = Self::keep(target, opened, wait, self.every, Rc::clone(&self.taken));
        keeping.push((partition, version, spawn(kept)));
    }

    /// The read took another frame: it has not stopped.
    pub(crate) fn took_frame(&self) {
        self.taken.set(self.taken.get().wrapping_add(1));
    }

    /// Takes a frame of a Scan of `target`'s version per `every`, from `wait` on, by a task of its own, so a pin that is refused or
    /// waits holds up no other. A pin refused holds nothing more: the read goes on, and a next Scan says what became of its version.
    /// A pin about to run out has the next opened first, while the read goes on taking frames, so a version held stays held; a reader
    /// that stopped holds its version no longer than one pin's grants.
    /// A dropped future, as its pins drop with their read, cancels each pin's Scan as its stream drops, which lets the
    /// version go.
    async fn keep(
        target: PinTarget,
        opened: Option<Result<ScanStream, ultragraphdb_protocol_wire::ErrorReply>>,
        wait: Duration,
        every: Duration,
        taken: Rc<Cell<u64>>,
    ) {
        compio::time::sleep(wait).await;
        let mut opened = opened.unwrap_or_else(|| target.open());
        loop {
            let Ok(mut pin) = opened else {
                return;
            };
            let seen = taken.get();
            for _ in 1..PinTarget::FRAMES {
                match pin.next(|_| Ok(())).await {
                    Ok(_) => compio::time::sleep(every).await,
                    Err(_) => return,
                }
            }
            if taken.get() == seen {
                return;
            }
            opened = target.open();
            // The last frame lets its version go, and the partition may discard past it on the same turn.
            if pin.next(|_| Ok(())).await.is_err() {
                return;
            }
        }
    }
}
