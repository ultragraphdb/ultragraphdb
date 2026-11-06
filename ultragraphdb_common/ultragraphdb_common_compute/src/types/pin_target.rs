/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{Scan, ScanAt, ScanRange};

use crate::types::link::Link;
use crate::types::scan_stream::ScanStream;
use crate::utils::keys::past_alone;

/// A version to keep.
#[derive(Debug)]
pub(super) struct PinTarget {
    pub(crate) link: Rc<Link>,
    pub(crate) tenant: u64,
    pub(crate) principal: u64,
    pub(crate) version: u64,
}

impl PinTarget {
    /// The empty ranges of a Scan holding a version: each answers one small frame, and a frame leaves per grant, so the Scan stays
    /// open for as many grants as it holds ranges, 160 s at the default `scan_stall_ms`. Few, since a Scan at once goes with most
    /// reads, each range a key's copy on the wire and at the partition, and a read outlasting it opens another.
    pub(crate) const FRAMES: usize = 16;

    pub(crate) fn open(&self) -> Result<ScanStream, ultragraphdb_protocol_wire::ErrorReply> {
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
        (0..Self::FRAMES).for_each(|_| {
            scan.push_range(ScanRange::new(self.link.partition(), &first, &past));
        });
        ScanStream::open(&self.link, &scan)
    }
}
