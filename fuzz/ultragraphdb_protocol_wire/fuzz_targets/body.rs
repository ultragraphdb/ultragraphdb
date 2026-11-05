/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a frame, read through its command's body view, the role commands' (512–767) among them: validation never panics; a body its view accepts reads
//! back to an owned body that builds the same frame, byte for byte, or, for the bodies whose strings and terms a
//! builder lays out canonically (Write, Describe, Result), a frame that builds itself again. An accepted frame holds what
//! its view promises: a commit's request fits its session; a Scan naming a transaction is at 1, waits for no epoch, names
//! no deadline and holds at most as many terms as ranges, and any other names one; a frame whose value continues has end 0
//! and an entry; a range 0 frame alone in its Scan's answer is taken when `LAST` says how it ended. A Prepare's request and
//! reply rules, which a view of a body sent both ways cannot hold alone, never panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    AbortView, AuthenticateView, BeginView, BindView, BodyBuilder, BodyView, CancelView,
    CatalogView, ChallengeView, CloseRequestView, ClosedView, Command, CommitView, ComputeReadView,
    ConditionalWriteView, CreditView, DefaultEntryView, DescribeView, DirectWriteView,
    DropCommandView, EntriesEnd, EntriesSequence, EntriesView, EpochCloseView, ErrorReplyView,
    ExecuteView, FLAG_LAST, FrameView, Header, HeldView, HelloView, InstallView, JoinView,
    LeaseView, MAX_FRAME_FLOOR, MAX_FRAME_SIZE, MembersView, OperationView, PlacementView,
    PrepareView, QueryView, ReadView, RecallView, ReferenceView, RegisterView, ReportView,
    ResultView, RoundView, ScanAt, ScanRange, ScanView, SubscribeView, VerifyView, WelcomeView,
    WriteView,
};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/body/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fn through<'a, V: BodyView<'a>>(frame: &FrameView<'a>) -> Result<Vec<u8>, ProtocolError> {
    let body = V::from_frame(frame)?.to_body()?;
    let h = frame.header();
    let header = Header::new(h.cluster(), Command::Abort)
        .with_sequence(h.sequence())
        .with_view(h.view())
        .with_release(h.release())
        .with_protocol(h.protocol())
        .with_stream(h.stream())
        .with_replica(h.replica())
        .with_flags(h.flags());
    body.frame(header)
}

fn rebuild(bytes: &[u8]) -> Result<(Command, Vec<u8>), ProtocolError> {
    let frame = FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?;
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
        Command::Authenticate => through::<AuthenticateView>(&frame),
        Command::Drop => through::<DropCommandView>(&frame),
        Command::DirectWrite => through::<DirectWriteView>(&frame),
        Command::ConditionalWrite => through::<ConditionalWriteView>(&frame),
        Command::Query => through::<QueryView>(&frame),
        Command::Install => through::<InstallView>(&frame),
        Command::Join => through::<JoinView>(&frame),
        Command::Members => through::<MembersView>(&frame),
        Command::Subscribe => through::<SubscribeView>(&frame),
        Command::EpochClose => through::<EpochCloseView>(&frame),
        Command::Lease => through::<LeaseView>(&frame),
        Command::Placement => through::<PlacementView>(&frame),
        Command::Catalog => through::<CatalogView>(&frame),
        Command::Challenge => through::<ChallengeView>(&frame),
        Command::Verify => through::<VerifyView>(&frame),
        Command::Bind => through::<BindView>(&frame),
        Command::Recall => through::<RecallView>(&frame),
        Command::ComputeRead => through::<ComputeReadView>(&frame),
        Command::Scan => through::<ScanView>(&frame),
        Command::Entries => through::<EntriesView>(&frame),
        Command::Reference => through::<ReferenceView>(&frame),
        Command::CloseRequest => through::<CloseRequestView>(&frame),
        Command::Closed => through::<ClosedView>(&frame),
        Command::DefaultEntry => through::<DefaultEntryView>(&frame),
        Command::Report => through::<ReportView>(&frame),
        Command::Held => through::<HeldView>(&frame),
    }?;
    match frame.command() {
        Command::Prepare => {
            let _ = PrepareView::from_frame(&frame).map(|v| (v.check_request(), v.check_reply()));
        }
        Command::Result => {
            let _ = ResultView::from_frame(&frame).map(|v| v.check_pattern());
        }
        Command::Query => {
            let _ = QueryView::from_frame(&frame).map(|v| v.check_text(MAX_FRAME_FLOOR as u32));
        }
        Command::DirectWrite => {
            let _ = DirectWriteView::from_frame(&frame).map(|v| v.check_from_client());
        }
        Command::ConditionalWrite => {
            let _ = ConditionalWriteView::from_frame(&frame).map(|v| v.check_from_client());
        }
        _ => {}
    }
    Ok((frame.command(), rebuilt))
}

/// What an accepted `frame`'s view promises; only its own command's view reads it.
fn holds_its_rules(frame: &FrameView<'_>) {
    let request_fits = |session: [u8; 16], request: u64| {
        assert_eq!(
            session == [0; 16],
            request == 0,
            "a request fits its session"
        );
    };
    if let Ok(view) = CommitView::from_frame(frame) {
        request_fits(view.session(), view.request());
        if view.session() == [0; 16] {
            assert!(view.token().is_none(), "a reply through no session");
        }
    }
    if let Ok(view) = DropCommandView::from_frame(frame) {
        request_fits(view.session(), view.request());
        if view.session() == [0; 16] {
            assert!(view.token().is_none(), "a reply through no session");
        }
    }
    if let Ok(view) = DirectWriteView::from_frame(frame) {
        request_fits(view.session(), view.request());
    }
    if let Ok(view) = ConditionalWriteView::from_frame(frame) {
        request_fits(view.session(), view.request());
    }
    if let Ok(view) = ScanView::from_frame(frame) {
        if view.transaction() != [0; 16] {
            assert_eq!(
                (view.at(), view.epoch(), view.deadline_ms()),
                (ScanAt::ReadVersion, 0, 0)
            );
            assert!(view.terms().len() <= view.range_count());
        } else {
            assert!(view.deadline_ms() >= 1);
        }
    }
    if let Ok(view) = EntriesView::from_frame(frame) {
        if view.continues() {
            assert_eq!(view.end(), EntriesEnd::More);
            assert!(!view.entries().is_empty());
        }
        assert!(view.through() >= view.range());
        assert!(view.through() == view.range() || view.end() == EntriesEnd::Ended);
        if view.range() == 0 && view.through() == 0 {
            let last = frame.header().flags() & FLAG_LAST != 0;
            // A range holding every key: the entries' places leave the rule of `LAST` to decide.
            assert_eq!(
                EntriesSequence::new([ScanRange::new(1, b"", b"")])
                    .accept(frame)
                    .is_ok(),
                last == (view.end() != EntriesEnd::More),
                "`LAST` says how the answer ended"
            );
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok((command, rebuilt)) = rebuild(data) else {
        return;
    };
    let frame = FrameView::validate(data, CLUSTER, MAX_FRAME_SIZE).expect("a frame just read");
    holds_its_rules(&frame);
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
