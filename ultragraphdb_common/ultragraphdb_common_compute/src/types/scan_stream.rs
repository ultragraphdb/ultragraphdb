/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{
    BodyView, Cancel, Command, Credit, EntriesEnd, EntriesSequence, EntriesView, ErrorReply,
    ErrorReplyView, FLAG_LAST, Scan,
};

use crate::types::link::Link;
use crate::types::read_fault::ReadFault;
use crate::utils::refusals::unavailable;

/// What one Entries frame said of its Scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanFacts {
    pub(crate) end: EntriesEnd,
    pub(crate) epoch: u64,
    pub(crate) version: u64,
    pub(crate) last: bool,
    pub(crate) continues: bool,
}

/// The stream of one Scan: its frames compared by an [`EntriesSequence`], credit granted one frame at a time as each is taken,
/// past the one frame a partition's Welcome starts a stream with, a stream given up cancelled.
#[derive(Debug)]
pub(crate) struct ScanStream {
    link: Rc<Link>,
    stream: u64,
    /// How long a frame may take once asked for: the Scan's wait for its epoch and the link's answer bound.
    wait: Duration,
    frames: UnboundedReceiver<ReceivedFrame>,
    sequence: EntriesSequence,
    took: bool,
    ended: bool,
}

impl ScanStream {
    /// Sends `scan` on `link`.
    pub(crate) fn open(link: &Rc<Link>, scan: &Scan) -> Result<Self, ErrorReply> {
        let (stream, frames) = link.open_stream()?;
        link.send(stream, scan)?;
        let wait =
            Duration::from_millis(u64::from(scan.deadline_ms())).saturating_add(link.answer_wait());
        Ok(Self {
            link: Rc::clone(link),
            stream,
            wait,
            frames,
            sequence: EntriesSequence::new(scan.ranges()),
            took: false,
            ended: false,
        })
    }

    /// The next frame, granting the credit the frame before it spent, read by `read` while its buffer lasts. A refusal of the
    /// partition's comes back as it was sent. A frame compute refuses closes the connection, and the read takes code 17; what the
    /// store holds that `read` cannot read takes this read alone, with code 17.
    pub(crate) async fn next<T>(
        &mut self,
        read: impl FnOnce(&EntriesView<'_>) -> Result<T, ReadFault>,
    ) -> Result<(ScanFacts, T), ErrorReply> {
        if self.took {
            self.link.send(self.stream, &Credit::new(1))?;
        }
        self.took = true;
        let Ok(next) = compio::time::timeout(self.wait, self.frames.next()).await else {
            // A partition silent past every bound it keeps is hung: what else waits on it is ended with the connection.
            self.ended = true;
            let why = format!(
                "partition {} sent no frame of a Scan within {:?}",
                self.link.partition(),
                self.wait
            );
            self.link.close(&why);
            return Err(unavailable(why));
        };
        let Some(frame) = next else {
            self.ended = true;
            return Err(self.link.closed_under_read());
        };
        let taken = self.taken(&frame, read);
        if let Err(Refused::Frame(why)) = &taken {
            self.ended = true;
            self.link.close(why);
        }
        match taken {
            Ok((facts, value)) => {
                self.ended = facts.last;
                Ok((facts, value))
            }
            Err(Refused::Partition(reply)) => {
                self.ended = true;
                Err(reply)
            }
            Err(Refused::Frame(why)) => Err(unavailable(format!(
                "a frame of partition {} was refused: {why}",
                self.link.partition()
            ))),
            Err(Refused::Stored(why)) => Err(unavailable(format!(
                "partition {} holds what does not read: {why}",
                self.link.partition()
            ))),
        }
    }

    /// What `frame` holds.
    fn taken<T>(
        &mut self,
        frame: &ReceivedFrame,
        read: impl FnOnce(&EntriesView<'_>) -> Result<T, ReadFault>,
    ) -> Result<(ScanFacts, T), Refused> {
        let view = frame
            .view()
            .map_err(|rule| Refused::Frame(rule.to_string()))?;
        if view.command() == Command::Error {
            let error = ErrorReplyView::from_frame(&view)
                .map_err(|rule| Refused::Frame(rule.to_string()))?;
            return Err(Refused::Partition(
                ErrorReply::new(error.code(), error.message()).with_detail(error.detail()),
            ));
        }
        let entries = self
            .sequence
            .accept(&view)
            .map_err(|rule| Refused::Frame(rule.to_string()))?;
        let value = read(&entries).map_err(|fault| match fault {
            ReadFault::Frame(why) => Refused::Frame(why),
            ReadFault::Stored(why) => Refused::Stored(why),
        })?;
        let facts = ScanFacts {
            end: entries.end(),
            epoch: entries.epoch(),
            version: entries.version(),
            last: view.header().flags() & FLAG_LAST != 0,
            continues: entries.continues(),
        };
        Ok((facts, value))
    }
}

/// Why a frame gave nothing.
enum Refused {
    /// The partition refused the Scan.
    Partition(ErrorReply),
    /// Compute refuses the frame.
    Frame(String),
    /// The frame's entries hold what the store cannot have written.
    Stored(String),
}

impl Drop for ScanStream {
    fn drop(&mut self) {
        if !self.ended {
            // A connection closed has cancelled every stream on it.
            drop(self.link.send(self.stream, &Cancel));
        }
        self.link.forget(self.stream);
    }
}
