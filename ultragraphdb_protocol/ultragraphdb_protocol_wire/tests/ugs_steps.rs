/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The wire-protocol scenarios of stage 6.6, `features/wire-protocol/ugs_frames.feature`: each frame built and read,
//! against frames laid out by hand in `utils/ugs_frames.rs` and the golden frames.

use cucumber::{then, when};
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyBuilder, BodyView, Command, CommitOperation, Describe, DescribeEntry,
    DescribeFrames, DropCommandView, EntryKind, FLAG_LAST, Header, Install, InstallView, Language,
    Operation, OperationView, Query, QueryView, RequestTermView, ResultView, TermTable, Warning,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::Steps;
use crate::utils::golden_bodies::{
    BODIES, CLUSTER, body_data, frame_of, golden, section, split_sections, with_command_header,
};
use crate::utils::golden_roles::{ROLES, role_bytes};
use crate::utils::ugs_frames::{
    UGS0902, UGS0902_MESSAGE, describe_frame, entry_sections, frame, install_reply, iri_terms,
    named, no_rest, query_frame, refused, result_frame, u32s, warning_sections,
};

/// The IRI the Operation scenarios name.
const INSTANCE: &str = "https://example.org/deep_brain#Node/ORG%3Aacme";

/// A source of 9,000 bytes, which frames of 4,096 bytes carry in three.
fn long_source() -> String {
    "namespace gen5 { }\n".repeat(474)
}

/// What the stage-6.6 scenarios carry from one step to the next.
#[derive(Debug, Default)]
pub struct UgsSteps {
    frames: Vec<Vec<u8>>,
    bytes: Vec<u8>,
    refusals: Vec<ProtocolErrorEnum>,
    accepted: Vec<bool>,
}

#[when("a tenant's Describe answer does not fit one frame within the client's `max_frame`")]
fn describe_past_max_frame(w: &mut Steps) -> TestResult {
    let answer = Describe::new(9, 42).with_entries(vec![DescribeEntry::new(
        EntryKind::Source,
        "gen5",
        long_source().into_bytes(),
    )]);
    w.ugs.frames = answer.frames(
        Header::new(CLUSTER, Command::Describe).with_stream(3),
        4_096,
    )?;
    Ok(())
}

#[then(
    "it arrives in continuation frames whose sections concatenate to the whole answer, the last flagged `LAST`"
)]
fn describe_continues(w: &mut Steps) -> TestResult {
    let frames = &w.ugs.frames;
    assert!(frames.len() > 1, "{} frames", frames.len());
    let mut joined = vec![Vec::new(); 9];
    let mut taken = DescribeFrames::new();
    for (index, bytes) in frames.iter().enumerate() {
        assert!(bytes.len() <= 4_096);
        let view = frame_of(bytes)?;
        let last = index + 1 == frames.len();
        assert_eq!(view.header().flags() & FLAG_LAST == FLAG_LAST, last);
        for (into, part) in joined
            .iter_mut()
            .zip(split_sections(body_data(bytes), 0, 9))
        {
            into.extend_from_slice(&part);
        }
        taken.push(&view)?;
    }
    let whole = describe_frame(
        &["gen5"],
        entry_sections(&[(19, 0, long_source().as_bytes())]),
        true,
    )?;
    assert_eq!(joined, split_sections(body_data(&whole), 0, 9));
    assert!(taken.is_last());
    let view = taken.view()?;
    assert_eq!(
        view.entry(0).context("the source")?.bytes(),
        long_source().as_bytes()
    );
    Ok(())
}

#[when("an Install reply with one warning `UGS0902` is built and read back")]
fn install_with_warning(w: &mut Steps) -> TestResult {
    let reply = Install::new(3, 11, vec![("s".to_string(), "t".to_string())])
        .installed(12, 44, "")
        .with_warnings(vec![
            Warning::new(UGS0902, UGS0902_MESSAGE).with_location(10..17),
        ]);
    w.ugs.bytes = reply.frame(
        Header::new(CLUSTER, Command::Install)
            .with_stream(3)
            .with_flags(FLAG_LAST),
    )?;
    // Built as the protocol lays it out, by hand.
    assert_eq!(
        w.ugs.bytes,
        install_reply(warning_sections(&[(UGS0902, 0, UGS0902_MESSAGE)]))?
    );
    Ok(())
}

#[then(
    "the view answers the version, the epoch, the report and one warning whose `diagnostic` is 902 in language 4, with its source, location and message"
)]
fn install_warning_read(w: &mut Steps) -> TestResult {
    let view = InstallView::from_frame(&frame_of(&w.ugs.bytes)?)?;
    assert_eq!((view.version(), view.epoch(), view.report()), (12, 44, ""));
    assert_eq!(view.warnings().len(), 1);
    let warning = view.warnings().get(0).context("the warning")?;
    assert_eq!((warning.number(), warning.language()), (902, 4));
    assert_eq!(
        (warning.source(), warning.offset(), warning.length()),
        (0, 10, 7)
    );
    assert_eq!(warning.message(), UGS0902_MESSAGE);
    Ok(())
}

#[when("an Install request's section 2 is not empty")]
fn install_request_with_report(w: &mut Steps) -> TestResult {
    let [ends, text] = crate::utils::ugs_frames::strings(&["s", "t"]);
    let request = frame(
        Command::Install,
        &[(0, &3u64.to_le_bytes()), (8, &11u64.to_le_bytes())],
        &[
            ends,
            text,
            b"report".to_vec(),
            vec![],
            vec![],
            vec![],
            vec![],
        ],
        false,
    )?;
    w.ugs.refusals = vec![refused::<InstallView>(&request)?];
    Ok(())
}

#[then("it is refused with `CountMismatch` naming `report`")]
fn report_refused(w: &mut Steps) -> TestResult {
    let refusal = w.ugs.refusals.first().context("a refusal")?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::CountMismatch {
                command: 23,
                field: "report",
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}

#[when("a Drop of kind 1 carries `catalog` 7")]
fn drop_predicate_with_catalog(w: &mut Steps) -> TestResult {
    // The golden Drop of a graph made a Drop of the predicate of that IRI, `catalog` at command-header offset 72.
    let predicate = with_command_header(&golden("drop")?.bytes()?, |h| {
        h[0] = 1;
        h[72] = 7;
    });
    w.ugs.refusals = vec![refused::<DropCommandView>(&predicate)?];
    Ok(())
}

#[then("it is refused with `FieldValue` naming `catalog`")]
fn catalog_refused(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.ugs.refusals,
        vec![ProtocolErrorEnum::FieldValue {
            command: 19,
            field: "catalog",
            value: 7
        }]
    );
    Ok(())
}

#[when(
    "a pattern's Result batch holds, in its reifier column, a term of tag `0x01` and another of tag `0x30`"
)]
fn reifier_column(w: &mut Steps) -> TestResult {
    let handle =
        "https://w3id.org/ultragraphdb/ns/tenant/0/MemberOf/00002a-0000000000000000000000000b";
    w.ugs.frames = vec![
        result_frame(1, 1, &[0], &[named(0x01, handle)], no_rest(), true)?,
        result_frame(
            1,
            1,
            &[0],
            &[(vec![0x30, 2, 1, b'7'], None)],
            no_rest(),
            true,
        )?,
    ];
    Ok(())
}

#[then("the first batch is read, and the second is refused with `FieldValue` naming `reifier`")]
fn reifier_checked(w: &mut Steps) -> TestResult {
    let [first, second] = w.ugs.frames.as_slice() else {
        return Err(ultragraphdb_tool_bdd::TestError::new("two batches"));
    };
    ResultView::from_frame(&frame_of(first)?)?.check_pattern()?;
    let view = ResultView::from_frame(&frame_of(second)?)?;
    assert_eq!(
        view.check_pattern().err().context("a refusal")?.0,
        ProtocolErrorEnum::FieldValue {
            command: 13,
            field: "reifier",
            value: 0
        }
    );
    Ok(())
}

/// The number of sections a golden frame's body holds: section 0 starts at the end of the table, `8 × N` rounded up.
fn section_count(bytes: &[u8]) -> usize {
    section(body_data(bytes), 0, 0).0 / 8
}

#[when(
    "the conformance suite reads every golden frame of Result, Prepare, Describe, Install and Reference"
)]
fn golden_frames_read(w: &mut Steps) -> TestResult {
    w.ugs.accepted.clear();
    // PROTOCOL.md: a Result of twelve sections (§9.1), a Prepare of ten (§8.1), a Describe of nine (§8.3), an Install
    // of seven (§8.4), a Reference of three (§13.15).
    let layouts = [
        ("result", 12),
        ("prepare", 10),
        ("describe", 9),
        ("install", 7),
    ];
    for body in BODIES {
        let Some((_, sections)) = layouts
            .iter()
            .find(|(prefix, _)| body.name.starts_with(prefix))
        else {
            continue;
        };
        let bytes = body.bytes()?;
        assert_eq!(section_count(&bytes), *sections, "{}", body.name);
        w.ugs.accepted.push(rebuilt_equals(&bytes)?);
    }
    for role in ROLES.iter().filter(|r| r.name.starts_with("reference")) {
        let bytes = role_bytes(role)?;
        assert_eq!(section_count(&bytes), 3, "{}", role.name);
        w.ugs.accepted.push(rebuilt_equals(&bytes)?);
    }
    Ok(())
}

/// Whether `bytes` decode through their command's view and build again to the same bytes.
fn rebuilt_equals(bytes: &[u8]) -> TestResult<bool> {
    let view = frame_of(bytes)?;
    let header = Header::new(CLUSTER, view.command())
        .with_sequence(view.header().sequence())
        .with_release(view.header().release())
        .with_stream(view.header().stream())
        .with_flags(view.header().flags());
    let rebuilt = match view.command() {
        Command::Result => ResultView::from_frame(&view)?.to_body()?.frame(header)?,
        Command::Prepare => ultragraphdb_protocol_wire::PrepareView::from_frame(&view)?
            .to_body()?
            .frame(header)?,
        Command::Describe => ultragraphdb_protocol_wire::DescribeView::from_frame(&view)?
            .to_body()?
            .frame(header)?,
        Command::Install => InstallView::from_frame(&view)?.to_body()?.frame(header)?,
        Command::Reference => ultragraphdb_protocol_wire::ReferenceView::from_frame(&view)?
            .to_body()?
            .frame(header)?,
        Command::Hello
        | Command::Welcome
        | Command::Register
        | Command::Begin
        | Command::Commit
        | Command::Abort
        | Command::Read
        | Command::Write
        | Command::Operation
        | Command::Execute
        | Command::Credit
        | Command::Cancel
        | Command::Error
        | Command::Round
        | Command::Authenticate
        | Command::Drop
        | Command::DirectWrite
        | Command::ConditionalWrite
        | Command::Query
        | Command::Purge
        | Command::Migrate
        | Command::Join
        | Command::Members
        | Command::Subscribe
        | Command::EpochClose
        | Command::Lease
        | Command::Placement
        | Command::Catalog
        | Command::Challenge
        | Command::Verify
        | Command::Bind
        | Command::Recall
        | Command::ComputeRead
        | Command::Scan
        | Command::Entries
        | Command::CloseRequest
        | Command::Closed
        | Command::DefaultEntry
        | Command::Report
        | Command::Held => {
            return Err(ultragraphdb_tool_bdd::TestError::new(format!(
                "a golden frame of {:?}",
                view.command()
            )));
        }
    };
    Ok(rebuilt == bytes)
}

#[then("each decodes, and building it again from its view gives the same bytes")]
fn golden_frames_rebuild(w: &mut Steps) -> TestResult {
    assert!(!w.ugs.accepted.is_empty());
    assert!(
        w.ugs.accepted.iter().all(|same| *same),
        "{:?}",
        w.ugs.accepted
    );
    Ok(())
}

#[when(
    "a Query with `language` 3, the text `Company { id }` and no parameters is built and read back"
)]
fn uql_query(w: &mut Steps) -> TestResult {
    let query = Query::new(5, Language::Uql, "Company { id }", TermTable::new());
    w.ugs.bytes = query.frame(Header::new(CLUSTER, Command::Query).with_stream(3))?;
    assert_eq!(w.ugs.bytes, query_frame(3, "Company { id }")?);
    Ok(())
}

#[then("the view answers language 3 and the text")]
fn uql_query_read(w: &mut Steps) -> TestResult {
    let view = QueryView::from_frame(&frame_of(&w.ugs.bytes)?)?;
    assert_eq!(view.language(), Language::Uql);
    assert_eq!(view.language().number(), 3);
    assert_eq!(view.text(), "Company { id }");
    assert_eq!(view.parameter_count(), 0);
    Ok(())
}

#[when("a Query names `language` 4")]
fn query_language_4(w: &mut Steps) -> TestResult {
    w.ugs.refusals = vec![refused::<QueryView>(&query_frame(4, "Company { id }")?)?];
    Ok(())
}

#[then("it is refused with `FieldValue` naming `language`")]
fn language_refused(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.ugs.refusals,
        vec![ProtocolErrorEnum::FieldValue {
            command: 22,
            field: "language",
            value: 4
        }]
    );
    Ok(())
}

#[when(
    "an Operation of kind 6 naming the default graph and `https://example.org/deep_brain#Node/ORG%3Aacme` is built and read back"
)]
fn retract_operation(w: &mut Steps) -> TestResult {
    let mut terms = TermTable::new();
    let instance = terms.iri(INSTANCE);
    let retract = Operation::new(
        terms,
        CommitOperation::RetractInstance {
            graph: None,
            instance,
        },
    );
    w.ugs.bytes = retract.frame(Header::new(CLUSTER, Command::Operation).with_stream(3))?;
    let [records, text] = iri_terms(&[INSTANCE]);
    let expected = frame(
        Command::Operation,
        &[(0, &[6])],
        &[u32s(&[ABSENT, 0]), records, text],
        false,
    )?;
    assert_eq!(w.ugs.bytes, expected);
    Ok(())
}

#[then("the view answers kind 6, the graph `ABSENT` and the IRI")]
fn retract_read(w: &mut Steps) -> TestResult {
    let view = OperationView::from_frame(&frame_of(&w.ugs.bytes)?)?;
    assert_eq!(view.kind(), 6);
    let CommitOperation::RetractInstance { graph, instance } = view.operation() else {
        return Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "a retraction: {:?}",
            view.operation()
        )));
    };
    assert_eq!(graph, None);
    assert_eq!(
        view.terms().get(instance).context("the instance")?,
        RequestTermView::Iri(INSTANCE)
    );
    Ok(())
}

#[when("an Operation of kind 5 carries three args")]
fn delete_with_three_args(w: &mut Steps) -> TestResult {
    let [records, text] = iri_terms(&[INSTANCE]);
    let bytes = frame(
        Command::Operation,
        &[(0, &[5])],
        &[u32s(&[ABSENT, 0, 0]), records, text],
        false,
    )?;
    w.ugs.refusals = vec![refused::<OperationView>(&bytes)?];
    Ok(())
}

#[then("it is refused with `CountMismatch` naming `args`")]
fn args_refused(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.ugs.refusals,
        vec![ProtocolErrorEnum::CountMismatch {
            command: 9,
            field: "args",
            count: 3,
            expected: 2
        }]
    );
    Ok(())
}
