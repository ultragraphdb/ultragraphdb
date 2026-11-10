/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of the frames a role and a client hold to the protocol's settled text: a Result's sections and refusals,
//! a Drop's kinds, a one-frame write's principal, Held, a ComputeRead's limits, a Scan's `after`, an Entries frame's run
//! and version, a Reference's failed store, an OperationFailed Error's detail.

use cucumber::{then, when};
use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_wire::{
    BeginView, BodyBuilder, BodyView, Command, ComputeReadView, DETAIL_VALUE_CUT, DirectWriteView,
    DropCommandView, Entries, EntriesEnd, EntriesSequence, EntriesView, ErrorCode, ErrorReply,
    ErrorReplyView, FLAG_LAST, FrameView, Header, HeldView, MAX_FRAME_FLOOR, MAX_FRAME_SIZE,
    RecordOperation, Reference, ReferenceRegistration, ReferenceView, ResultBatch, ResultView,
    ScanRange, ScanView, TermTable, WelcomeView, WriteSetRecord, WriteSetRecordView,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::utils::golden_bodies::{
    CLUSTER, body_data, golden, put, section, with_command_header, with_section,
};
use crate::utils::golden_roles::{golden_role, role_bytes};

/// An epoch of these frames, and its version bound.
const EPOCH: u64 = 30;
const BOUND: u64 = (EPOCH << 22) | 0x3F_FFFF;
/// A version of `EPOCH` whose sequence is odd.
const ODD: u64 = (EPOCH << 22) | 1;

fn frame_of(bytes: &[u8]) -> TestResult<FrameView<'_>> {
    Ok(FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE)?)
}

fn role(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

fn refused<'a, V: BodyView<'a>>(bytes: &'a [u8]) -> TestResult<ProtocolError> {
    let frame = frame_of(bytes)?;
    V::from_frame(&frame).err().context("a refusal")
}

/// The name a `FieldValue` refusal gives, or an error for any other refusal.
fn field_named(refusal: &ProtocolError) -> TestResult<&'static str> {
    if let ProtocolErrorEnum::FieldValue { field, .. } = refusal.0 {
        return Ok(field);
    }
    Err(TestError::new(format!("not a field's refusal: {refusal}")))
}

fn fields(w: &Steps) -> TestResult<Vec<&'static str>> {
    w.refusals.iter().map(field_named).collect()
}

// A Result's sections and refusals.

#[when(
    "a Result's `monotonic` is 3, and another's sections 7 and 8 name a column count other than its `columns`"
)]
fn result_flags(w: &mut Steps) -> TestResult {
    let bytes = golden("result")?.bytes()?;
    let flagged = with_command_header(&bytes, |h| h[20] = 3);
    let named = golden("result_query")?.bytes()?;
    let one_name = with_section(&named, 12, 7, |ends| ends.truncate(4));
    let one_name = with_section(&one_name, 12, 8, |text| text.truncate(1));
    w.refusals = vec![
        refused::<ResultView>(&flagged)?,
        refused::<ResultView>(&one_name)?,
    ];
    Ok(())
}

#[then(
    "the receiver refuses the first with `FieldValue` naming `monotonic` and the second with `CountMismatch` naming `column names`"
)]
fn result_flags_refused(w: &mut Steps) -> TestResult {
    let [flag, names] = w.refusals.as_slice() else {
        return Err(TestError::new("two refusals"));
    };
    assert_eq!(field_named(flag)?, "monotonic");
    assert!(matches!(
        names.0,
        ProtocolErrorEnum::CountMismatch {
            field: "column names",
            count: 1,
            expected: 2,
            ..
        }
    ));
    Ok(())
}

#[when("a Result frame not flagged `LAST` holds a resume key")]
fn resume_before_the_last(w: &mut Steps) -> TestResult {
    let golden = golden("result")?;
    let bytes = ResultBatch::new(1)
        .with_resume(b"rk".to_vec())
        .frame(golden.header().with_flags(0))?;
    w.refusals = vec![refused::<ResultView>(&bytes)?];
    Ok(())
}

#[then("the receiver refuses it with `FieldValue` naming `resume key`")]
fn resume_refused(w: &mut Steps) -> TestResult {
    assert_eq!(fields(w)?, ["resume key"]);
    Ok(())
}

#[when("a Result's term encodes an IRI by hash, its `text` `ABSENT`")]
fn iri_without_text(w: &mut Steps) -> TestResult {
    // Term 0 of the golden batch is an IRI, its text the first of the section's pairs.
    let bytes = with_section(&golden("result")?.bytes()?, 12, 3, |texts| {
        put(texts, 0, u32::MAX.into(), 4);
    });
    w.refusals = vec![refused::<ResultView>(&bytes)?];
    Ok(())
}

#[then("the receiver refuses it with `FieldValue` naming `term text`, the term's index")]
fn term_text_refused(w: &mut Steps) {
    assert!(matches!(
        w.refusals.as_slice(),
        [ProtocolError(ProtocolErrorEnum::FieldValue {
            field: "term text",
            value: 0,
            ..
        })]
    ));
}

// A Drop's kinds.

#[when("a Drop's `kind` is 5, and a write-set record names operation 5")]
fn kind_five(w: &mut Steps) -> TestResult {
    let drop = with_command_header(&golden("drop")?.bytes()?, |h| h[0] = 5);
    let mut record = WriteSetRecord::build(3, RecordOperation::DropTenant).encode();
    let (operations, _) = section(&record, 64, 3);
    put(&mut record, operations, 5, 2);
    w.refusals = vec![
        refused::<DropCommandView>(&drop)?,
        WriteSetRecordView::new(&record)
            .err()
            .context("a refusal")?,
    ];
    Ok(())
}

#[then(
    "the Drop is refused with `FieldValue` naming `kind`, the record with `FieldValue` naming `operation`, and nothing is dropped"
)]
fn kind_five_refused(w: &mut Steps) -> TestResult {
    assert_eq!(fields(w)?, ["kind", "operation"]);
    Ok(())
}

// A one-frame write's principal.

#[when("a client's DirectWrite holds a `principal` above 0")]
fn client_principal(w: &mut Steps) -> TestResult {
    let bytes = golden("direct_write_relayed")?.bytes()?;
    let view = DirectWriteView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.principal(), 9);
    w.refusals = vec![
        DirectWriteView::from_client_frame(&frame_of(&bytes)?)
            .err()
            .context("a refusal")?,
    ];
    let sent = golden("direct_write")?.bytes()?;
    let _sent = DirectWriteView::from_client_frame(&frame_of(&sent)?)?;
    Ok(())
}

#[then(
    "it is refused at the client endpoint with `FieldValue` naming `principal`, and no partition sees it"
)]
fn principal_refused(w: &mut Steps) -> TestResult {
    assert_eq!(fields(w)?, ["principal"]);
    Ok(())
}

// Held.

#[when(
    "a Held frame arrives on a stream whose request's answers do not list it, or carries a nonzero command-header byte"
)]
fn held_elsewhere(w: &mut Steps) -> TestResult {
    let held = role("held")?;
    let view = frame_of(&held)?;
    let _held: HeldView<'_> = HeldView::from_frame(&view)?;
    assert_eq!(view.command().answering(Command::Write)?, Command::Held);
    assert_eq!(view.command().answering(Command::Operation)?, Command::Held);
    let bad = with_command_header(&held, |h| h[127] = 1);
    w.refusals = vec![
        view.command()
            .answering(Command::Commit)
            .err()
            .context("a refusal")?,
        refused::<HeldView>(&bad)?,
    ];
    Ok(())
}

#[then("the receiver refuses it as `CommandNotServed`, or with `ReservedNotZero`")]
fn held_refused(w: &mut Steps) {
    assert_eq!(
        w.refusals,
        [
            ProtocolError::command_not_served(Command::Held.number()),
            ProtocolError::reserved_not_zero(128 + 127),
        ]
    );
}

// A Welcome's credit and a ComputeRead's limits.

#[when("a Welcome names `initial_credit` 0")]
fn welcome_no_credit(w: &mut Steps) -> TestResult {
    let bytes = with_command_header(&golden("welcome")?.bytes()?, |h| put(h, 16, 0, 4));
    w.refusals = vec![refused::<WelcomeView>(&bytes)?];
    Ok(())
}

#[then("it is refused with `FieldValue` naming `initial_credit`")]
fn no_credit_refused(w: &mut Steps) -> TestResult {
    assert_eq!(fields(w)?, ["initial_credit"]);
    Ok(())
}

#[when("a ComputeRead names `max_frame` 524,287 or 1,048,577, or `credit` 0")]
fn compute_read_limits(w: &mut Steps) -> TestResult {
    let bytes = role("compute_read")?;
    let (below, above) = (MAX_FRAME_FLOOR as u64 - 1, MAX_FRAME_SIZE as u64 + 1);
    w.refusals = [
        with_command_header(&bytes, |h| put(h, 64, below, 4)),
        with_command_header(&bytes, |h| put(h, 64, above, 4)),
        with_command_header(&bytes, |h| put(h, 68, 0, 4)),
    ]
    .iter()
    .map(|frame| refused::<ComputeReadView>(frame))
    .collect::<TestResult<_>>()?;
    Ok(())
}

#[then("it is refused with `FieldValue` naming `max_frame` or `credit`")]
fn limits_refused(w: &mut Steps) -> TestResult {
    assert_eq!(fields(w)?, ["max_frame", "max_frame", "credit"]);
    Ok(())
}

// A Scan's `after`.

#[when("a Scan's range holds `after_len` 16 and the object's encoding after its past key")]
fn scan_after(w: &mut Steps) -> TestResult {
    w.bytes = role("scan_after")?;
    Ok(())
}

#[then(
    "the view reads the first key, the past key and the `after` borrowed from the frame, and the next range's first key starts where the `after` ends"
)]
fn after_read(w: &mut Steps) -> TestResult {
    let frame = frame_of(&w.bytes)?;
    let view = ScanView::from_frame(&frame)?;
    let (first, second) = (
        view.range(0).context("range 0")?,
        view.range(1).context("range 1")?,
    );
    let after = first.after();
    assert_eq!(after.len(), 16);
    assert_eq!(first.first(), b"\x10abc");
    assert_eq!(first.past(), b"\x10abd");
    assert_eq!(second.first(), b"\x10abd");
    assert!(second.after().is_empty());
    let body = body_data(&w.bytes);
    let at = |key: &[u8]| key.as_ptr() as usize - body.as_ptr() as usize;
    assert_eq!(at(after), at(first.past()) + first.past().len());
    assert_eq!(at(second.first()), at(after) + after.len());
    Ok(())
}

// An Entries frame's run and version.

#[when(
    "an Entries frame holds `range` 2, `through` 5, `end` 1 and two entries, and a frame with `through` above `range` and `end` 0"
)]
fn entries_run(w: &mut Steps) -> TestResult {
    w.bytes = role("entries_run")?;
    let more = with_command_header(&w.bytes, |h| h[8] = 0);
    w.refusals = vec![refused::<EntriesView>(&more)?];
    Ok(())
}

#[then(
    "the first is read as ranges 2 to 5, each ended, and the second is refused with `FieldValue` naming `through`"
)]
fn run_read(w: &mut Steps) -> TestResult {
    let frame = frame_of(&w.bytes)?;
    let view = EntriesView::from_frame(&frame)?;
    assert_eq!((view.range(), view.through()), (2, 5));
    assert_eq!(view.end(), EntriesEnd::Ended);
    assert_eq!(view.entries().len(), 2);
    assert_eq!(fields(w)?, ["through"]);
    Ok(())
}

#[when(
    "an Entries frame's `version`, a Scan's `read_version` and a Begin's `read_version` each hold an odd sequence, and then each holds an epoch's version bound"
)]
fn version_fields(w: &mut Steps) -> TestResult {
    let entries = role("entries")?;
    let scan = role("scan")?;
    let begin = golden("begin_named")?.bytes()?;
    w.frames = vec![
        with_command_header(&entries, |h| put(h, 24, ODD, 8)),
        with_command_header(&scan, |h| put(h, 8, ODD, 8)),
        with_command_header(&begin, |h| put(h, 8, ODD, 8)),
        with_command_header(&entries, |h| put(h, 24, BOUND, 8)),
        with_command_header(&scan, |h| put(h, 8, BOUND, 8)),
        with_command_header(&begin, |h| put(h, 8, BOUND, 8)),
    ];
    Ok(())
}

#[then("the first three are refused with `FieldValue` naming the field, and the bound is read")]
fn versions_read(w: &mut Steps) -> TestResult {
    let [entries, scan, begin, entries_bound, scan_bound, begin_bound] = w.frames.as_slice() else {
        return Err(TestError::new("six frames"));
    };
    assert_eq!(field_named(&refused::<EntriesView>(entries)?)?, "version");
    assert_eq!(field_named(&refused::<ScanView>(scan)?)?, "read_version");
    assert_eq!(field_named(&refused::<BeginView>(begin)?)?, "read_version");
    assert_eq!(
        EntriesView::from_frame(&frame_of(entries_bound)?)?.version(),
        BOUND
    );
    assert_eq!(
        ScanView::from_frame(&frame_of(scan_bound)?)?.read_version(),
        BOUND
    );
    assert_eq!(
        BeginView::from_frame(&frame_of(begin_bound)?)?.read_version(),
        BOUND
    );
    Ok(())
}

#[when("a Scan's second Entries frame names another `version` than its first")]
fn another_version(w: &mut Steps) -> TestResult {
    let header = Header::new(CLUSTER, Command::Entries);
    let first = Entries::new(0, EntriesEnd::More, EPOCH).with_version(BOUND);
    let second = Entries::new(0, EntriesEnd::Ended, EPOCH).with_version(BOUND - 1);
    let (first, second) = (
        first.frame(header.clone())?,
        second.frame(header.with_flags(FLAG_LAST))?,
    );
    let mut sequence = EntriesSequence::new([ScanRange::new(1, b"\x10", b"")]);
    let _first = sequence.accept(&frame_of(&first)?)?;
    let frame = frame_of(&second)?;
    w.refusals = vec![sequence.accept(&frame).err().context("a refusal")?];
    Ok(())
}

#[then("the receiver refuses it with `FieldValue` naming `version`, and closes the connection")]
fn another_version_refused(w: &mut Steps) -> TestResult {
    assert_eq!(fields(w)?, ["version"]);
    Ok(())
}

#[when(
    "a run's entry holds a key that is the first key of none of its ranges, or a range's entry lies below the range's first key"
)]
fn outside_its_run(w: &mut Steps) -> TestResult {
    let (key, other) = (b"\x1b\x01".as_slice(), b"\x1b\x02".as_slice());
    let (key_past, other_past) = ([key, &[0]].concat(), [other, &[0]].concat());
    let run = [
        ScanRange::new(1, key, &key_past),
        ScanRange::new(1, other, &other_past),
    ];
    let header = Header::new(CLUSTER, Command::Entries).with_flags(FLAG_LAST);
    let mut batch = Entries::new(0, EntriesEnd::Ended, EPOCH).with_through(1);
    batch.push(other, b"id");
    batch.push(b"\x1b\x03", b"id");
    let mut below = Entries::new(0, EntriesEnd::Ended, EPOCH);
    below.push(b"\x10a", b"");
    w.refusals = vec![
        sequence_refusal(run, &batch.frame(header.clone())?)?,
        sequence_refusal([ScanRange::new(1, b"\x10b", b"")], &below.frame(header)?)?,
    ];
    Ok(())
}

/// The refusal of the frame `bytes` by a sequence of a Scan of `ranges`.
fn sequence_refusal<'r>(
    ranges: impl IntoIterator<Item = ScanRange<'r>>,
    bytes: &[u8],
) -> TestResult<ProtocolError> {
    let frame = frame_of(bytes)?;
    EntriesSequence::new(ranges)
        .accept(&frame)
        .err()
        .context("a refusal")
}

#[then(
    "the receiver refuses it with `FieldValue` naming `entry order`, the entry's index, and closes the connection"
)]
fn outside_refused(w: &mut Steps) {
    let entry_order =
        |value| ProtocolError::field_value(Command::Entries.number(), "entry order", value);
    assert_eq!(w.refusals, [entry_order(1), entry_order(0)]);
}

// A Reference's failed store.

#[when("a home's store fails under a registration")]
fn failed_store(w: &mut Steps) -> TestResult {
    let group = BucketGroup::new(1)?;
    let entity = SystemId::decode(&golden_entity())?;
    let registration = ReferenceRegistration::request(3, Some(group), None, Some(entity), 4)
        .refused(ErrorCode::Unavailable.number());
    let reply = Reference::new(true, vec![registration], TermTable::new());
    w.bytes = reply.frame(golden_role("reference_reply")?.header())?;
    Ok(())
}

/// A system ID's 16 bytes: type prefix 1, sequence 7, bucket 9.
fn golden_entity() -> [u8; 16] {
    [0, 1, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 9, 0, 0, 0]
}

#[then("the reply's `outcome` is 17, its `generation` and `close` 0, and a receiver accepts it")]
fn failed_store_accepted(w: &mut Steps) -> TestResult {
    let frame = frame_of(&w.bytes)?;
    let view = ReferenceView::from_frame(&frame)?;
    let registration = view.registrations().next().context("a registration")?;
    assert_eq!(registration.outcome(), ErrorCode::Unavailable.number());
    assert_eq!((registration.generation(), registration.close()), (0, 0));
    Ok(())
}

// An OperationFailed Error's detail.

#[when(
    "an Error of code 8 arrives with `detail` 1, another with bit 1 set, and another with bit 63 set"
)]
fn operation_failed_details(w: &mut Steps) -> TestResult {
    let bytes = golden("error_operation")?.bytes()?;
    w.frames = [DETAIL_VALUE_CUT, 1 << 1, 1 << 63]
        .map(|detail| with_command_header(&bytes, |h| put(h, 8, detail, 8)))
        .to_vec();
    Ok(())
}

#[then(
    "the receiver accepts the first, its value cut, and refuses the others with `FieldValue` naming `detail`; a sender builds neither"
)]
fn operation_failed_details_checked(w: &mut Steps) -> TestResult {
    let [cut, others @ ..] = w.frames.as_slice() else {
        return Err(TestError::new("the three frames"));
    };
    let view = ErrorReplyView::from_frame(&frame_of(cut)?)?;
    assert_eq!(view.code(), ErrorCode::OperationFailed);
    assert!(view.value_cut());
    assert_eq!(others.len(), 2);
    for (bytes, detail) in others.iter().zip([1u64 << 1, 1 << 63]) {
        let refusal = ProtocolError::field_value(Command::Error.number(), "detail", detail);
        assert_eq!(refused::<ErrorReplyView>(bytes)?, refusal);
        let built = ErrorReply::new(ErrorCode::OperationFailed, "found another")
            .with_detail(detail)
            .frame(golden("error_operation")?.header());
        assert_eq!(built.err(), Some(refusal));
    }
    Ok(())
}
