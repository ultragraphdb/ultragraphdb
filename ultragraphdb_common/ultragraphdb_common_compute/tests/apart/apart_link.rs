/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A core's link to one partition driven alone, through a [`LinkProbe`], against a partition double that takes every request
//! and answers as the test does, behind a tap when a test asks for one (design D2). A frame the double sends on a stream other
//! than the one it answers is carried on an open stream of the double's, whose queue sends a frame as it is: the frame's own
//! header names where it lands.

use std::rc::Rc;
use std::time::Duration;

use ultragraphdb_common_compute::{DoubleFrame, LinkProbe, RoleDouble, Told};
use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyBuilder, Command, Entries, EntriesEnd, ErrorReply, FLAG_LAST, Header, Scan,
    ScanAt, ScanRange,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::apart_cluster::{CLUSTER, even_map, links_listing, played_role_cluster};
use super::apart_partition::{EPOCH, VERSION};
use super::apart_tap::ApartTap;

/// How long a test waits for what it expects to be told.
pub const WAIT: Duration = Duration::from_secs(5);

/// How long a test waits to see that nothing more is told.
pub const QUIET: Duration = Duration::from_millis(200);

/// How often a test looks again at what it waits for.
pub const LOOK: Duration = Duration::from_millis(2);

/// The partition every probed link reaches.
pub const PARTITION: u32 = 1;

/// A probed link to a partition double that takes every request and answers none of its own, and the tap in front of the
/// double when one was asked for.
pub struct Probed {
    pub double: RoleDouble,
    pub tap: Option<Rc<ApartTap>>,
    pub probe: LinkProbe,
}

/// A partition double holding every request open, the probe's link listed at it, or at a tap in front of it when `tapped`.
pub async fn probed(tapped: bool) -> TestResult<Probed> {
    let cluster = played_role_cluster()?;
    let double = RoleDouble::start(&cluster, Endpoint::Partition, |_| Answer::Open).await?;
    let address = Address::parse(double.address())?;
    let tap = if tapped {
        Some(ApartTap::start(&address).await?)
    } else {
        None
    };
    let dialled = tap
        .as_ref()
        .map_or_else(|| address.to_string(), |tap| tap.address().to_string());
    let links = links_listing(&cluster, even_map(1)?, &[(PARTITION, dialled)]);
    Ok(Probed {
        double,
        tap,
        probe: LinkProbe::new(links, PARTITION),
    })
}

/// `body` as a frame of `stream`, flagged last when `last`.
pub fn frame_on<B: BodyBuilder>(stream: u64, body: &B, last: bool) -> TestResult<Vec<u8>> {
    let header = Header::new(CLUSTER, B::COMMAND).with_stream(stream);
    let header = if last {
        header.with_flags(FLAG_LAST)
    } else {
        header
    };
    Ok(body.frame(header)?)
}

/// A Scan of partition [`PARTITION`] from `first`, at its latest version.
pub fn scan_from(first: &[u8]) -> Scan {
    let mut scan = Scan::new(ScanAt::Latest, 0, 0, 0, [0; 16], 0, 0);
    scan.push_range(ScanRange::new(PARTITION, first, &[]));
    scan
}

/// A Scan of the partition's first keys.
pub fn scan() -> Scan {
    scan_from(b"\x01")
}

/// A transaction's Bind to [`PARTITION`].
pub fn bind() -> Bind {
    Bind::new(PARTITION, BindKind::Transaction, 0, 0, 0, [7; 16])
}

/// An Entries frame of one range on `stream`, more to follow unless `last`.
pub fn entries_on(stream: u64, last: bool) -> TestResult<Vec<u8>> {
    let end = if last {
        EntriesEnd::Ended
    } else {
        EntriesEnd::More
    };
    frame_on(
        stream,
        &Entries::new(0, end, EPOCH).with_version(VERSION),
        last,
    )
}

/// The stream a read's request opened, its refusal a test's failure.
pub fn opened(stream: Result<u64, ErrorReply>) -> TestResult<u64> {
    stream.map_err(|refusal| TestError::new(format!("the request was refused: {refusal:?}")))
}

/// Opens a Bind stream on the probe's link: its number.
pub fn open_bind(probe: &mut LinkProbe) -> TestResult<u64> {
    let stream = probe.next_stream().context("a link for the partition")?;
    probe
        .open(frame_on(stream, &bind(), false)?)
        .context("the Bind's stream")
}

/// Waits until `double` took a request of `command` on `stream`: that request, which the test answers.
pub async fn taken(double: &RoleDouble, command: Command, stream: u64) -> TestResult<DoubleFrame> {
    let found = || {
        double
            .frames(command)
            .into_iter()
            .find(|frame| frame.stream() == stream)
    };
    let waited = async {
        // Each look waits a little; the wait bounds them.
        loop {
            if let Some(frame) = found() {
                return frame;
            }
            compio::time::sleep(LOOK).await;
        }
    };
    compio::time::timeout(WAIT, waited)
        .await
        .map_err(|_| TestError::new(format!("the double took no {command:?} on stream {stream}")))
}

/// The next stream told something and what, within [`WAIT`].
pub async fn told(probe: &mut LinkProbe) -> TestResult<(u64, Told)> {
    compio::time::timeout(WAIT, probe.next_told())
        .await
        .map_err(|_| TestError::new(format!("nothing was told within {WAIT:?}")))
}

/// What is told within [`QUIET`]: nothing, when the link has nothing more to tell.
pub async fn told_quietly(probe: &mut LinkProbe) -> Option<(u64, Told)> {
    compio::time::timeout(QUIET, probe.next_told()).await.ok()
}

/// Every stream told `Closed` within [`WAIT`], in the order told, until `count` were: the test asserts which.
pub async fn closes(probe: &mut LinkProbe, count: usize) -> TestResult<Vec<u64>> {
    let mut closed = Vec::new();
    while closed.len() < count {
        let (stream, what) = told(probe).await?;
        if what != Told::Closed {
            return Err(TestError::new(format!(
                "stream {stream} was told {what:?} before the close"
            )));
        }
        closed.push(stream);
    }
    closed.sort_unstable();
    Ok(closed)
}
