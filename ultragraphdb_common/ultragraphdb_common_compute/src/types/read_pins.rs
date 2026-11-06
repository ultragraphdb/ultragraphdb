/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{JoinHandle, spawn};
use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{Scan, ScanAt, ScanRange};

use crate::types::link::Link;
use crate::types::scan_stream::ScanStream;
use crate::utils::keys::past_alone;

/// The empty ranges of a Scan holding a version: each answers one small frame, and a frame leaves per grant, so the Scan stays
/// open for as many grants as it holds ranges.
const PIN_RANGES: usize = 512;

/// The versions a read has chosen, one per partition: what keeps them readable across a read that outlasts `read_retain_ms`
/// (`PROTOCOL.md` §11.9, quad-reads "A partition retains what a read may still read"). A short read never opens a Scan for them.
#[derive(Debug, Default)]
pub(crate) struct ReadPins {
    targets: RefCell<Vec<PinTarget>>,
    taken: Cell<u64>,
}

/// A version to keep.
#[derive(Debug)]
struct PinTarget {
    link: Rc<Link>,
    tenant: u64,
    principal: u64,
    version: u64,
}

impl ReadPins {
    /// `version` of the partition of `link` is what the read reads there.
    pub(crate) fn add(&self, link: &Rc<Link>, tenant: u64, principal: u64, version: u64) {
        self.targets.borrow_mut().push(PinTarget {
            link: Rc::clone(link),
            tenant,
            principal,
            version,
        });
    }

    /// The read took another frame: it has not stopped.
    pub(crate) fn took_frame(&self) {
        self.taken.set(self.taken.get().wrapping_add(1));
    }

    /// Holds every version the read has chosen once `after` has passed, each by a task of its own, so a pin that is refused or
    /// waits holds up no other, for as long as the task running this lives: a read ending drops the tasks, which cancels each Scan.
    pub(crate) async fn hold(pins: Rc<Self>, after: Duration, every: Duration) {
        compio::time::sleep(after).await;
        let mut keeping: Vec<JoinHandle<()>> = Vec::new();
        loop {
            let chosen = pins.targets.borrow().len();
            let new = keeping.len()..chosen;
            keeping.extend(new.map(|at| spawn(Self::keep(Rc::clone(&pins), at, every))));
            compio::time::sleep(every).await;
        }
    }

    /// Takes a frame of a Scan of version `at` per `every`. A pin refused holds nothing more: the read goes on, and a next Scan says
    /// what became of its version. A pin run out is opened again while the read goes on taking frames, so a reader that stopped
    /// holds its version no longer than one pin's grants.
    async fn keep(pins: Rc<Self>, at: usize, every: Duration) {
        loop {
            let opened = pins.targets.borrow().get(at).map(PinTarget::open);
            let Some(Ok(mut pin)) = opened else {
                return;
            };
            let taken = pins.taken.get();
            loop {
                match pin.next(|_| Ok(())).await {
                    Ok((facts, ())) if facts.last => break,
                    Ok(_) => compio::time::sleep(every).await,
                    Err(_) => return,
                }
            }
            if pins.taken.get() == taken {
                return;
            }
        }
    }
}

impl PinTarget {
    fn open(&self) -> Result<ScanStream, ultragraphdb_protocol_wire::ErrorReply> {
        let first = key_prefix(Keyspace::Referrers, false, self.tenant, None, None);
        let past = past_alone(&first);
        let mut scan = Scan::new(
            ScanAt::ReadVersion,
            self.version,
            0,
            self.principal,
            [0; 16],
            1,
            0,
        );
        (0..PIN_RANGES).for_each(|_| {
            scan.push_range(ScanRange::new(self.link.partition(), &first, &past));
        });
        ScanStream::open(&self.link, &scan)
    }
}
