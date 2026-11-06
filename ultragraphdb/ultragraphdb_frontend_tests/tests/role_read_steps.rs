/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client's reads through the real frontend, compute and storage: what a result names its terms by.

use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Carrier;
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, BodyView, Command, DirectWrite, EntriesView, FLAG_LAST, Header,
    PatternSlot, QuadPattern, Read, RequestTerm, ResultView, Scan, ScanAt, ScanRange, TermTable,
    WriteChange,
};
use ultragraphdb_store_quads::{iri_bucket, term_key};
use ultragraphdb_tests::{
    CLUSTER, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, FRONTEND_BINARY, FRONTEND_OBSERVABILITY,
    SECRET, STORAGE_BINARY, STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, answering,
    compute_on_one_host, holding, role, scratch, storage_holding, until_answering,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::endpoint_steps::{Authenticated, bootstrapping, client_address, until_listening};
use crate::{Steps, services};

const TENANT: u64 = 0;
const PARTITION: u32 = 1;
const KNOWS: &str = "http://ex.org/knows";
const A: &str = "http://ex.org/a";
const B: &str = "http://ex.org/b";
const GENID: &str = "/.well-known/genid/";

/// The term of a Result, and the bytes of the frames that held it.
#[derive(Debug, Default)]
pub(crate) struct Answered {
    cells: Vec<(String, bool)>,
    bytes: Vec<u8>,
}

impl Answered {
    /// The text a blank term of the answer holds.
    fn blank(&self) -> TestResult<&str> {
        self.cells
            .iter()
            .find_map(|(text, blank)| blank.then_some(text.as_str()))
            .context("a blank term")
    }

    fn names(&self, text: &str) -> bool {
        self.cells
            .iter()
            .any(|(held, blank)| held == text && !*blank)
    }

    fn holds(&self, needle: &[u8]) -> bool {
        self.bytes
            .windows(needle.len())
            .any(|window| window == needle)
    }
}

/// Starts the three binaries over one partition: the administrator's connection to the frontend.
async fn connected() -> TestResult<Authenticated<Carrier>> {
    let carrier = Carrier::connect(&client_address()?).await?;
    Authenticated::over(carrier).await
}

fn started(w: &mut Steps) -> TestResult {
    let util = services()?;
    let frontend = answering(FRONTEND_OBSERVABILITY, "/health");
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &bootstrapping()?,
        &scratch(FRONTEND_BINARY),
        frontend,
    ))?;
    let _storage: u32 = util.start_service_from_config(role(
        STORAGE_BINARY,
        &storage_holding()?,
        &scratch(STORAGE_BINARY),
        answering(STORAGE_OBSERVABILITY, "/health"),
    ))?;
    let _compute: u32 = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &holding(compute_on_one_host(), SECRET)?,
        &scratch(COMPUTE_BINARY),
        answering(COMPUTE_OBSERVABILITY, "/health"),
    ))?;
    w.roles = Some(util);
    until_listening()?;
    until_answering(STORAGE_OBSERVABILITY, "/ready")?;
    until_answering(COMPUTE_OBSERVABILITY, "/ready")
}

/// A DirectWrite of tenant 0 holding `change`, its terms `terms`.
async fn wrote(
    client: &mut Authenticated<Carrier>,
    stream: u64,
    terms: TermTable,
    change: WriteChange,
) -> TestResult {
    let write = DirectWrite::new(TENANT, terms, vec![change]);
    let answer = client.asked(stream, &write).await?;
    if answer == Command::Commit {
        Ok(())
    } else {
        Err(TestError::new(format!("the write answered {answer:?}")))
    }
}

async fn inserted(client: &mut Authenticated<Carrier>, stream: u64, subject: &str) -> TestResult {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(KNOWS),
        object: terms.iri(B),
    };
    wrote(client, stream, terms, change).await
}

/// One read of `<knows>` in a read-only transaction on streams from `stream`: its rows' terms and the frames' bytes.
async fn read_once(client: &mut Authenticated<Carrier>, stream: u64) -> TestResult<Answered> {
    let answer = client.asked(stream, &Begin::new(TENANT, 0, true)).await?;
    if answer != Command::Begin {
        return Err(TestError::new(format!("the Begin answered {answer:?}")));
    }
    let mut terms = TermTable::new();
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Bound(terms.iri(KNOWS)),
        PatternSlot::Unbound,
    );
    let read = Read::new(stream, 16, terms, pattern, Vec::new());
    let read_stream = stream + 2;
    client
        .send(read.frame(Header::new(CLUSTER, Command::Read).with_stream(read_stream))?)
        .await?;
    let mut answered = Answered::default();
    loop {
        let frame = client.next().await?;
        if frame_ends(&frame, &mut answered)? {
            return Ok(answered);
        }
    }
}

/// Adds `frame`, a Result, to `answered`: whether it was the read's last.
fn frame_ends(frame: &ReceivedFrame, answered: &mut Answered) -> TestResult<bool> {
    let view = frame.view()?;
    let result = ResultView::from_frame(&view)?;
    answered.bytes.extend_from_slice(frame.bytes());
    for row in 0..result.rows() {
        for column in 0..result.columns() {
            let term = result.cell(column, row).and_then(|cell| result.term(cell));
            if let Some(term) = term {
                let text = term.text().unwrap_or_default().to_string();
                answered.cells.push((text, term.is_blank_node()));
            }
        }
    }
    Ok(view.header().flags() & FLAG_LAST != 0)
}

/// Reads until a read's epoch holds what was written: read-only reads answer at a closed epoch.
async fn read_rows(client: &mut Authenticated<Carrier>, from: u64) -> TestResult<Answered> {
    for attempt in 0..100u64 {
        let answered = read_once(client, from + attempt * 4).await?;
        if !answered.cells.is_empty() {
            return Ok(answered);
        }
        compio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(TestError::new("no read saw the write"))
}

/// The bytes of the system ID the partition holds for `iri`.
async fn system_id(iri: &str) -> TestResult<Vec<u8>> {
    let address = Address::parse(&format!("tcp://{STORAGE_ROLE_ADDRESS}"))?;
    let mut client = RoleClient::dial(&address, CLUSTER, "frontend", SECRET).await?;
    let key = term_key(TENANT, iri);
    let past = [key.as_slice(), &[0]].concat();
    let mut scan = Scan::new(ScanAt::Latest, 0, 0, 0, [0; 16], 0, 0);
    scan.push_range(ScanRange::new(PARTITION, &key, &past));
    client.send_only(&scan).await?;
    let frame = client.next_frame().await?;
    let view = frame.view()?;
    let entries = EntriesView::from_frame(&view)?;
    let found = entries
        .entries()
        .iter()
        .next()
        .and_then(|entry| entry.value().map(<[u8]>::to_vec));
    found.context("the term's entry")
}

#[when("a read returns rows whose subjects are `http://ex.org/a` and `http://ex.org/b`")]
fn two_subjects(w: &mut Steps) -> TestResult {
    started(w)?;
    let (answered, ids) = Runtime::new()?.block_on(async {
        let mut client = connected().await?;
        inserted(&mut client, 1, A).await?;
        inserted(&mut client, 3, B).await?;
        let answered = read_rows(&mut client, 5).await?;
        let ids = vec![system_id(A).await?, system_id(B).await?];
        TestResult::Ok((answered, ids))
    })?;
    w.answered = answered;
    w.ids = ids;
    Ok(())
}

#[then(
    "the batch's dictionary holds both IRIs' texts and the rows name them by dictionary index, with no system ID"
)]
fn two_named(w: &mut Steps) -> TestResult {
    for iri in [A, B] {
        assert!(w.answered.names(iri), "{iri}: {:?}", w.answered.cells);
    }
    no_system_id(w);
    w.roles = None;
    Ok(())
}

fn no_system_id(w: &Steps) {
    for id in &w.ids {
        assert!(!w.answered.holds(id), "a system ID {id:?} in a result");
    }
}

#[when("a read returns a row whose subject is a blank node")]
fn blank_subject(w: &mut Steps) -> TestResult {
    started(w)?;
    w.answered = Runtime::new()?.block_on(async {
        let mut client = connected().await?;
        let mut terms = TermTable::new();
        let subject = terms.intern(RequestTerm::BlankNode("x".to_string()))?;
        let change = WriteChange::Insert {
            graph: None,
            subject,
            predicate: terms.iri(KNOWS),
            object: terms.iri(B),
        };
        wrote(&mut client, 1, terms, change).await?;
        read_rows(&mut client, 3).await
    })?;
    Ok(())
}

#[then(
    "the batch names it, tagged as a blank node, by its skolem IRI `…/.well-known/genid/{bucket}-{random}`, whose text holds no type prefix and no sequence of its system ID"
)]
fn blank_named(w: &mut Steps) -> TestResult {
    let skolem = w.answered.blank()?;
    let tail = skolem
        .split_once(GENID)
        .map(|(_, tail)| tail)
        .context("the skolem path")?;
    assert_eq!(tail.split('-').count(), 2, "{skolem}");
    let bucket = iri_bucket(TENANT, skolem);
    assert!(bucket.is_some(), "{skolem} states its bucket");
    w.roles = None;
    Ok(())
}

#[when("a client reads the subject of a statement whose subject is `http://ex.org/a`")]
fn one_subject(w: &mut Steps) -> TestResult {
    started(w)?;
    let (answered, ids) = Runtime::new()?.block_on(async {
        let mut client = connected().await?;
        inserted(&mut client, 1, A).await?;
        let answered = read_rows(&mut client, 3).await?;
        let ids = vec![system_id(A).await?];
        TestResult::Ok((answered, ids))
    })?;
    w.answered = answered;
    w.ids = ids;
    Ok(())
}

#[then("the result batch names `http://ex.org/a` by its text, and its bytes contain no system ID")]
fn one_named(w: &mut Steps) {
    assert!(w.answered.names(A), "{:?}", w.answered.cells);
    assert!(w.answered.holds(A.as_bytes()));
    no_system_id(w);
    w.roles = None;
}

#[when("a client reads a statement that carries a property and so holds a statement ID")]
fn a_statement(w: &mut Steps) -> TestResult {
    started(w)?;
    let (answered, ids) = Runtime::new()?.block_on(async {
        let mut client = connected().await?;
        let mut terms = TermTable::new();
        let since = (
            terms.iri("http://ex.org/since"),
            terms.iri("http://ex.org/2020"),
        );
        let change = WriteChange::InsertEdge {
            graph: None,
            subject: terms.iri(A),
            predicate: terms.iri(KNOWS),
            object: terms.iri(B),
            properties: vec![since],
        };
        wrote(&mut client, 1, terms, change).await?;
        let answered = read_rows(&mut client, 3).await?;
        let ids = vec![system_id(A).await?, system_id(B).await?];
        TestResult::Ok((answered, ids))
    })?;
    w.answered = answered;
    w.ids = ids;
    Ok(())
}

#[then(
    "the result names the statement by its quad and its reifier's skolem IRI, whose text holds the statement's bucket and no sequence, and its bytes contain no system ID"
)]
fn statement_named(w: &mut Steps) -> TestResult {
    // The pattern binds the predicate, so a row states the rest of the quad.
    for iri in [A, B] {
        assert!(w.answered.names(iri), "{iri}: {:?}", w.answered.cells);
    }
    let reifier = w.answered.blank()?;
    let tail = reifier
        .split_once(GENID)
        .map(|(_, tail)| tail)
        .context("the skolem path")?;
    assert_eq!(tail.split('-').count(), 2, "{reifier}");
    assert_eq!(iri_bucket(TENANT, reifier), iri_bucket(TENANT, A));
    no_system_id(w);
    w.roles = None;
    Ok(())
}
