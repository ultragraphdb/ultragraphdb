/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, ResultBatch, ResultTerm,
};

use crate::types::pushed::Pushed;

/// Where a read's Result batches go: each built within the read's `max_frame`, one at a time, the next only once the one before
/// has left and the stream holds credit (`PROTOCOL.md` §13.12).
#[derive(Debug)]
pub(crate) struct ResultSink {
    cluster: u64,
    stream: u64,
    max_frame: usize,
    columns: u32,
    read_at: (u64, u32),
    batch: ResultBatch,
    pushes: UnboundedSender<Pushed>,
    room: UnboundedReceiver<()>,
}

impl ResultSink {
    /// The sink of a read on `stream` of frames of at most `max_frame` bytes and of `columns` columns.
    pub(crate) fn new(
        (cluster, stream): (u64, u64),
        max_frame: u32,
        columns: u32,
        pushes: UnboundedSender<Pushed>,
        room: UnboundedReceiver<()>,
    ) -> Self {
        Self {
            cluster,
            stream,
            max_frame: max_frame as usize, // lossless: a `usize` is at least 32 bits
            columns,
            read_at: (0, 0),
            batch: ResultBatch::new(columns),
            pushes,
            room,
        }
    }

    /// Every batch from here on was read at `epoch`, `partition` the one the read reached alone.
    pub(crate) fn read_at(&mut self, epoch: u64, partition: u32) {
        self.read_at = (epoch, partition);
        self.batch = self.empty_batch();
    }

    fn empty_batch(&self) -> ResultBatch {
        ResultBatch::new(self.columns).with_read_at(self.read_at.0, self.read_at.1)
    }

    /// The Error refusing a row or key too large for its frame.
    fn too_large(&self, what: &str) -> ErrorReply {
        ErrorReply::new(
            ErrorCode::TooLarge,
            format!(
                "a {what} is too large for a frame of {} bytes",
                self.max_frame
            ),
        )
        .with_detail(self.max_frame as u64) // lossless: a `usize` is at most 64 bits
    }

    /// Adds `row` to the batch, sending the batch first when the row does not fit beside it.
    /// A future dropped while it waits for room leaves the batch sent, and the room it waited for stays for the next
    /// wait.
    pub(crate) async fn push_row(&mut self, row: &[Option<&ResultTerm>]) -> Result<(), ErrorReply> {
        if self.batch.frame_size_with(row, 0) > self.max_frame {
            if self.batch.rows() > 0 {
                self.send_batch(false, Vec::new()).await?;
            }
            if self.batch.frame_size_with(row, 0) > self.max_frame {
                let mut alone = self.empty_batch();
                alone
                    .push_row(row)
                    .map_err(|rule| ErrorReply::protocol(&rule))?;
                if !self.fits_exactly(&alone)? {
                    return Err(self.too_large("row"));
                }
            }
        }
        self.batch
            .push_row(row)
            .map_err(|rule| ErrorReply::protocol(&rule))
    }

    /// Whether `batch` builds to a frame within the read's `max_frame`.
    fn fits_exactly(&self, batch: &ResultBatch) -> Result<bool, ErrorReply> {
        match batch.frame(self.header(false)) {
            Ok(frame) => Ok(frame.len() <= self.max_frame),
            Err(_) => Ok(false),
        }
    }

    /// Ends the read's stream with what is held and `resume`, alone in a last batch of `rows` 0 when it does not fit beside it.
    /// A future dropped while it waits for room leaves the batch sent.
    pub(crate) async fn finish(mut self, resume: Vec<u8>) -> Result<(), ErrorReply> {
        if self.batch.frame_size_with(&[], resume.len()) > self.max_frame {
            let any_rows = self.batch.rows() > 0;
            if any_rows {
                self.send_batch(false, Vec::new()).await?;
            }
            if self.batch.frame_size_with(&[], resume.len()) > self.max_frame {
                return Err(self.too_large("resume key"));
            }
        }
        self.send_batch(true, resume).await
    }

    fn header(&self, last: bool) -> Header {
        let header = Header::new(self.cluster, Command::Result).with_stream(self.stream);
        if last {
            header.with_flags(FLAG_LAST)
        } else {
            header
        }
    }

    /// Sends the batch held, and waits for room for the next unless it was the stream's last.
    /// A future dropped at its wait leaves the batch sent, and the room it waited for stays for the next wait.
    async fn send_batch(&mut self, last: bool, resume: Vec<u8>) -> Result<(), ErrorReply> {
        let next = self.empty_batch();
        let batch = std::mem::replace(&mut self.batch, next).with_resume(resume);
        let frame = batch
            .frame(self.header(last))
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        drop(batch);
        self.pushes
            .unbounded_send(Pushed::Frame(self.stream, frame))
            .map_err(|_| ErrorReply::new(ErrorCode::Unavailable, "the read's connection closed"))?;
        if !last && self.room.next().await.is_none() {
            return Err(ErrorReply::new(
                ErrorCode::Unavailable,
                "the read's connection closed",
            ));
        }
        Ok(())
    }
}
