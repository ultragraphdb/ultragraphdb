/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a frame, read through its command's body view: validation never panics; a body its view accepts reads
//! back to an owned body that builds the same frame, byte for byte, or, for the bodies whose strings and terms a
//! builder lays out canonically (Write, Describe, Result), a frame that builds itself again.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    AbortView, BeginView, BodyBuilder, BodyView, CancelView, Command, CommitView, CreditView,
    DescribeView, ErrorReplyView, ExecuteView, FrameView, Header, HelloView, OperationView,
    PrepareView, ReadView, RegisterView, ResultView, RoundView, WelcomeView, WriteView,
};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/body/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fn through<'a, V: BodyView<'a>>(frame: &FrameView<'a>) -> Result<Vec<u8>, ProtocolError> {
    let body = V::from_frame(frame)?.to_body()?;
    let h = frame.header();
    let header = Header::new(h.cluster(), Command::Abort)
        .with_view(h.view())
        .with_release(h.release())
        .with_protocol(h.protocol())
        .with_stream(h.stream())
        .with_replica(h.replica())
        .with_flags(h.flags());
    body.frame(header)
}

fn rebuild(bytes: &[u8]) -> Result<(Command, Vec<u8>), ProtocolError> {
    let frame = FrameView::validate(bytes, CLUSTER)?;
    let rebuilt = match frame.command() {
        Command::Hello => through::<HelloView>(&frame),
        Command::Welcome => through::<WelcomeView>(&frame),
        Command::Register => through::<RegisterView>(&frame),
        Command::Begin => through::<BeginView>(&frame),
        Command::Commit => through::<CommitView>(&frame),
        Command::Abort => through::<AbortView>(&frame),
        Command::Read => through::<ReadView>(&frame),
        Command::Write => through::<WriteView>(&frame),
        Command::Operation => through::<OperationView>(&frame),
        Command::Prepare => through::<PrepareView>(&frame),
        Command::Execute => through::<ExecuteView>(&frame),
        Command::Describe => through::<DescribeView>(&frame),
        Command::Result => through::<ResultView>(&frame),
        Command::Credit => through::<CreditView>(&frame),
        Command::Cancel => through::<CancelView>(&frame),
        Command::Error => through::<ErrorReplyView>(&frame),
        Command::Round => through::<RoundView>(&frame),
    }?;
    Ok((frame.command(), rebuilt))
}

fuzz_target!(|data: &[u8]| {
    let Ok((command, rebuilt)) = rebuild(data) else {
        return;
    };
    if matches!(
        command,
        Command::Write | Command::Describe | Command::Result
    ) {
        let (_, again) = rebuild(&rebuilt).expect("a rebuilt body reads");
        assert_eq!(again, rebuilt, "{command:?} rebuilds to itself");
    } else {
        assert_eq!(rebuilt, data, "{command:?} has one encoding");
    }
});
