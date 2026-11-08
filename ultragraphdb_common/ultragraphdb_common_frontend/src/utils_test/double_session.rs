/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::rc::Rc;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use ultragraphdb_common_role::{Answer, ParkedBytes, Session};
use ultragraphdb_protocol_transport::ReceivedFrame;
use ultragraphdb_protocol_wire::{Command, ErrorCode, ErrorReply};

use crate::utils_test::double_frame::DoubleFrame;

/// The session of each connection a [`crate::RoleDouble`] accepts: it notes every frame and answers as its script says.
pub(crate) struct DoubleSession<F> {
    pub seen: Rc<RefCell<Vec<DoubleFrame>>>,
    pub script: Rc<F>,
    pub pushes: UnboundedSender<(u64, Vec<u8>)>,
    pub pushed: UnboundedReceiver<(u64, Vec<u8>)>,
    pub reading: ParkedBytes,
}

impl<F> DoubleSession<F> {
    fn note(&self, stream: u64, command: Command, bytes: Vec<u8>) -> DoubleFrame {
        let frame = DoubleFrame::new(stream, command, bytes, self.pushes.clone());
        self.seen.borrow_mut().push(frame.clone());
        frame
    }
}

impl<F: Fn(&DoubleFrame) -> Answer> Session for DoubleSession<F> {
    /// It holds no `.await`, so a dropped future leaves nothing.
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        let Ok(view) = frame.view() else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        let noted = self.note(stream, view.command(), frame.bytes().to_vec());
        (self.script)(&noted)
    }

    fn parked(&self) -> Option<ParkedBytes> {
        Some(self.reading.clone())
    }

    /// A future dropped at its wait leaves the channel as it was, and the next call takes what came meanwhile.
    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        match self.pushed.next().await {
            Some(pushed) => pushed,
            // This session holds a sender, so its channel never ends while it waits here.
            None => std::future::pending().await,
        }
    }

    fn cancelled(&mut self, stream: u64) {
        let _noted = self.note(stream, Command::Cancel, Vec::new());
    }

    fn credited(&mut self, stream: u64, _frames: u32) {
        let _noted = self.note(stream, Command::Credit, Vec::new());
    }
}
