/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Structure-aware: bodies of every kind, their term tables included, built from arbitrary fields within each
//! builder's rules, which must validate and read back to what was built; a Describe answer and a Catalog version also cut
//! into continuation frames within a `max_frame` (`PROTOCOL.md` §8.3, §13.7), each frame within it and the last alone
//! flagged `LAST`, which their assemblers take back to what was built. Then arbitrary mutations of the body, of one
//! section with the body laid out again around it and of the command header, the frame rebuilt around them so the body's
//! checks are reached, which never panic. A DirectWrite, ConditionalWrite, Scan, Entries, Purge, Migrate or Catalog the
//! mutations leave acceptable builds the same frame, byte for byte, and holds what its view promises.

#![no_main]

use std::fmt::Debug;
use std::num::NonZeroU32;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Authenticate, AuthenticateView, BodyBuilder, BodyView, Catalog, CatalogFrames, CatalogView,
    ColumnType, Command, Commit, CommitOperation, CommitView, ConditionalWrite,
    ConditionalWriteView, Credit, CreditView, Describe, DescribeEntry, DescribeFrames,
    DescribeView, DirectWrite, DirectWriteView, Direction, DropCommand, DropCommandView, Entries,
    EntriesEnd, EntriesView, EntryKind, ErrorCode, ErrorReply, ErrorReplyView, Execute,
    ExecuteView, FLAG_LAST, FrameView, Header, Hello, HelloView, Install, InstallView, Language,
    Link, MAX_EPOCH, MAX_FRAME_FLOOR, MAX_FRAME_SIZE, Migrate, MigrateAction, MigrateState,
    MigrateView, Operation, OperationResult, OperationView, PURGE_IRIS_MAX, PatternSlot, Prepare,
    PrepareView, Purge, PurgeView, QuadPattern, Query, QueryView, Read, ReadView, RequestTerm,
    ResultBatch, ResultTerm, ResultView, RetiredItem, RetiredName, RoundBody, RoundView, Scan,
    ScanAt, ScanRange, ScanView, StatementTarget, TermTable, WARNING_MESSAGE_MAX, WARNINGS_MAX,
    Warning, Welcome, WelcomeView, Write, WriteChange, WriteSetRecord, WriteView,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A term from arbitrary fields: a triple term names earlier entries of the table it is interned into.
#[derive(Debug, Arbitrary)]
enum Term {
    Iri(String),
    BlankNode(String),
    Literal(String, Option<String>),
    LanguageLiteral(String, String, u8),
    Triple(u16, u16, u16),
}

#[derive(Debug, Arbitrary)]
enum Slot {
    Unbound,
    Bound(u16),
    Parameter,
}

/// A commit-time operation's fields, as [`commit_operation`] reads them.
#[derive(Debug, Arbitrary)]
struct OperationFields {
    kind: u8,
    args: [u16; 4],
    delta: i64,
    links: Vec<(u16, bool)>,
}

/// A change's fields, as [`change`] reads them.
#[derive(Debug, Arbitrary)]
struct ChangeFields {
    kind: u8,
    subject: u16,
    predicate: u16,
    object: u16,
}

/// A Scan range's fields, as [`scan`] reads them.
#[derive(Debug, Arbitrary)]
struct RangeFields {
    partition: u32,
    first: Vec<u8>,
    past: Vec<u8>,
    after: Vec<u8>,
}

/// A Scan's fields, as [`scan`] reads them.
#[derive(Debug, Arbitrary)]
struct ScanFields {
    at: u8,
    read_version: u64,
    epoch: u64,
    principal: u64,
    /// `None` for a Scan naming no transaction.
    transaction: Option<[u8; 16]>,
    limit: u32,
    deadline_ms: u32,
    ranges: Vec<RangeFields>,
    iris: Vec<String>,
}

/// A warning's fields, as [`warnings`] reads them.
#[derive(Debug, Arbitrary)]
struct WarningFields {
    number: u16,
    language: u8,
    source: u32,
    offset: u32,
    length: u32,
    message: String,
}

/// A Describe entry's fields, as [`describe_entries`] reads them: `bytes` for kinds 1 to 18, `text` a source's, and
/// `retired` a retired name's item, whether it was renamed, its current name, version and window.
#[derive(Debug, Arbitrary)]
struct EntryFields {
    kind: u8,
    name: String,
    bytes: Vec<u8>,
    text: String,
    retired: (u8, bool, String, u64, u64),
}

#[derive(Debug, Arbitrary)]
enum Body {
    Hello {
        min: u16,
        extra: u8,
        release: u32,
        max_frame: u32,
        name: String,
    },
    Welcome {
        protocol: u16,
        max_frame: u32,
        streams: u32,
        credit: u32,
        write_set: u32,
        transaction_ms: u32,
        name: String,
        mechanisms: Vec<String>,
    },
    Authenticate {
        step: u16,
        mechanism: String,
        message: Vec<u8>,
        last: Option<(u64, u64)>,
    },
    Drop {
        kind: u8,
        tenant: u64,
        target: String,
        session: [u8; 16],
        request: u64,
        version: u64,
        /// The catalog version a drop of kind 3 or 4 names.
        catalog: u64,
    },
    Commit {
        session: [u8; 16],
        request: u64,
        version: u64,
        results: Vec<(u64, bool)>,
    },
    Credit {
        frames: u32,
    },
    Error {
        code: u8,
        detail: u64,
        message: String,
        value: Option<String>,
    },
    Read {
        transaction: u64,
        limit: u32,
        slots: [Slot; 4],
        resume: Vec<u8>,
    },
    Prepare {
        slots: [Slot; 4],
        node_parameters: bool,
    },
    /// A Prepare of a text, language 1 to 3, its parameters' names and types; a reply's statement, schema id and
    /// projections, with its warnings.
    PrepareText {
        language: u8,
        text: String,
        parameters: Vec<(String, u8)>,
        answered: Option<(u64, u64, Vec<(String, u8)>)>,
        warnings: Vec<WarningFields>,
    },
    Execute {
        statement: u64,
        transaction: u64,
        parameters: Vec<u16>,
        resume: Vec<u8>,
    },
    Write {
        changes: Vec<(u8, u16, u16, u16)>,
    },
    Operation(OperationFields),
    /// A cell's odd value names a blank node, an even one an IRI, the last column, the reifier column, among them.
    Result {
        columns: u8,
        rows: Vec<Vec<Option<u16>>>,
    },
    /// A query's first batch: its column names, rows of IRIs, blank nodes and the values of tags `0x41`, `0x50` and
    /// `0x51`, and its warnings.
    QueryResult {
        names: Vec<String>,
        rows: Vec<Vec<Option<u16>>>,
        warnings: Vec<WarningFields>,
    },
    Round {
        decisions: Vec<Option<(u8, u64)>>,
        keys: Vec<Vec<u8>>,
    },
    DirectWrite {
        session: [u8; 16],
        request: u64,
        tenant: u64,
        token: Option<u64>,
        changes: Vec<ChangeFields>,
    },
    ConditionalWrite {
        session: [u8; 16],
        request: u64,
        tenant: u64,
        token: Option<u64>,
        changes: Vec<ChangeFields>,
        operations: Vec<OperationFields>,
    },
    Scan(ScanFields),
    Entries {
        range: u32,
        /// Past `range` for a run, which ends every range.
        run: u16,
        end: u8,
        continues: bool,
        epoch: u64,
        version: u64,
        entries: Vec<(Vec<u8>, Vec<u8>)>,
    },
    Query {
        transaction: u64,
        language: u8,
        limit: u32,
        text: String,
        parameters: Vec<(String, u16)>,
        resume: Vec<u8>,
    },
    Install {
        tenant: u64,
        version: u64,
        sources: Vec<(String, String)>,
        /// A reply's version, the epoch it takes effect at and its report.
        answered: Option<(u64, u64, String)>,
        /// A reply's staged namespace and its migration report, and its warnings.
        staged: (u64, String),
        warnings: Vec<WarningFields>,
    },
    Describe {
        tenant: u64,
        read_version: u64,
        entries: Vec<EntryFields>,
        max_frame: u16,
    },
    Purge {
        tenant: u64,
        iris: Vec<String>,
        /// A reply's epoch and the instances it removed.
        purged: Option<(u64, u32)>,
    },
    Migrate {
        tenant: u64,
        staged: u64,
        action: u8,
        /// A reply's state, epoch and report, and a failed run's refusal and message.
        answered: Option<(u8, u64, String, Option<(u8, String)>)>,
    },
    Catalog {
        tenant: u64,
        version: u64,
        /// A reply's epoch, position and bytes.
        answered: Option<(u64, u64, Vec<u8>)>,
        max_frame: u16,
    },
}

#[derive(Debug, Arbitrary)]
enum Mutation {
    Flip {
        at: u16,
        bit: u8,
    },
    Truncate {
        len: u16,
    },
    Append {
        byte: u8,
    },
    Set {
        at: u16,
        value: u32,
    },
    /// A byte of the command header, `at % 128`, set to a value.
    CommandHeader {
        at: u8,
        value: u8,
    },
    /// One section of a body laid out in sections alone changed, and the body laid out again around it, so a rule of the
    /// section's contents is reached past the section table's.
    Section {
        index: u8,
        change: SectionChange,
    },
}

#[derive(Debug, Arbitrary)]
enum SectionChange {
    Clear,
    Truncate {
        len: u16,
    },
    Append {
        byte: u8,
    },
    Set {
        at: u16,
        value: u8,
    },
    /// The u32 at a byte, `at` modulo the section's length, set to all ones: an absent arg's or value's mark.
    Absent {
        at: u16,
    },
}

#[derive(Debug, Arbitrary)]
struct Input {
    terms: Vec<Term>,
    body: Body,
    stream: u64,
    mutations: Vec<Mutation>,
}

/// The table of `terms`, each interned or, when it is refused, left out; at least one IRI, so indices have a term.
fn table(terms: &[Term]) -> TermTable {
    let mut table = TermTable::new();
    table.iri("http://ex.org/0");
    for term in terms {
        let len = table.len() as u32;
        let term = match term {
            Term::Iri(s) => RequestTerm::Iri(s.clone()),
            Term::BlankNode(s) => RequestTerm::BlankNode(s.clone()),
            Term::Literal(lexical, datatype) => RequestTerm::Literal {
                lexical: lexical.clone(),
                datatype: datatype.clone(),
            },
            Term::LanguageLiteral(lexical, language, direction) => RequestTerm::LanguageLiteral {
                lexical: lexical.clone(),
                language: language.clone(),
                direction: match direction % 3 {
                    0 => None,
                    1 => Some(Direction::Ltr),
                    _ => Some(Direction::Rtl),
                },
            },
            Term::Triple(s, p, o) => RequestTerm::Triple {
                subject: u32::from(*s) % len,
                predicate: u32::from(*p) % len,
                object: u32::from(*o) % len,
            },
        };
        let _ = table.intern(term);
    }
    table
}

fn slot(s: &Slot, parameters: &mut u32, terms: u32) -> PatternSlot {
    match s {
        Slot::Unbound => PatternSlot::Unbound,
        Slot::Bound(n) => PatternSlot::Bound(u32::from(*n) % terms),
        Slot::Parameter => {
            *parameters += 1;
            PatternSlot::Parameter(*parameters - 1)
        }
    }
}

/// The change `fields` name, of each kind a change takes, over the term indices `t` gives.
fn change(fields: &ChangeFields, t: &impl Fn(u16) -> u32) -> WriteChange {
    let (subject, predicate, object) = (t(fields.subject), t(fields.predicate), t(fields.object));
    let graph = (fields.kind & 8 != 0).then_some(object);
    let statement = if fields.kind & 16 != 0 {
        StatementTarget::Triple {
            graph,
            subject,
            predicate,
            object,
        }
    } else {
        StatementTarget::Reifier {
            graph,
            reifier: subject,
        }
    };
    match fields.kind % 8 {
        0 | 1 => WriteChange::Insert {
            graph,
            subject,
            predicate,
            object,
        },
        2 => WriteChange::Delete {
            graph,
            subject,
            predicate,
            object,
        },
        3 => WriteChange::InsertEdge {
            graph,
            subject,
            predicate,
            object,
            properties: vec![(predicate, object)],
        },
        4 => WriteChange::DeleteStatement {
            graph,
            reifier: subject,
        },
        5 => WriteChange::DeletePattern {
            graph,
            subject,
            predicate,
        },
        6 => WriteChange::SetProperty {
            statement,
            predicate,
            value: object,
        },
        _ => WriteChange::DeleteProperty {
            statement,
            predicate,
            value: (fields.kind & 32 != 0).then_some(object),
        },
    }
}

/// The operation `fields` name, of each kind, over the term indices `t` gives.
fn commit_operation(fields: &OperationFields, t: &impl Fn(u16) -> u32) -> CommitOperation {
    let [a, b, c, d] = fields.args;
    let graph = (fields.delta % 2 == 0).then(|| t(c));
    match fields.kind % 6 {
        0 => CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: t(a),
            value: t(b),
            properties: vec![(t(c), t(d))],
        },
        1 => CommitOperation::UpsertAndLink {
            graph: None,
            predicate: t(a),
            value: t(b),
            properties: vec![],
            links: fields
                .links
                .iter()
                .map(|(x, from)| {
                    if *from {
                        Link::from_node(t(*x), t(*x))
                    } else {
                        Link::to_node(t(*x), t(*x))
                    }
                })
                .collect(),
        },
        2 => CommitOperation::CompareAndSet {
            graph: Some(t(a)),
            subject: t(b),
            predicate: t(c),
            expected: t(d),
            new: t(a),
        },
        3 => CommitOperation::CounterAdd {
            graph: None,
            subject: t(a),
            predicate: t(b),
            delta: fields.delta,
        },
        4 => CommitOperation::DeleteInstance {
            graph,
            instance: t(d),
        },
        _ => CommitOperation::RetractInstance {
            graph,
            instance: t(d),
        },
    }
}

/// The warnings `fields` name, at most [`WARNINGS_MAX`]: each diagnostic's language 1 to 4, its location within a u32,
/// its message at most [`WARNING_MESSAGE_MAX`] bytes, cut on a character boundary, and its source 0 unless `sourced`.
fn warnings(fields: &[WarningFields], sourced: bool) -> Vec<Warning> {
    fields
        .iter()
        .take(WARNINGS_MAX)
        .map(|w| {
            let diagnostic = ((1 + u32::from(w.language % 4)) << 16) | u32::from(w.number);
            let mut end = w.message.len().min(WARNING_MESSAGE_MAX);
            while !w.message.is_char_boundary(end) {
                end -= 1;
            }
            Warning::new(diagnostic, &w.message[..end])
                .with_source(if sourced { w.source } else { 0 })
                .with_location(w.offset..w.offset.saturating_add(w.length))
        })
        .collect()
}

/// The entries `fields` name, each kind 1 to 20, in the order of a catalog version's entries, by kind, then by name as
/// UTF-8 bytes, each kind and name once (`PROTOCOL.md` §8.3).
fn describe_entries(fields: &[EntryFields]) -> Vec<DescribeEntry> {
    let mut entries: Vec<DescribeEntry> = fields
        .iter()
        .filter_map(|f| {
            let kind = EntryKind::from_number(1 + f.kind % 20)?;
            Some(match kind {
                EntryKind::Source => DescribeEntry::new(kind, &f.name, f.text.clone().into_bytes()),
                EntryKind::RetiredName => {
                    let (item, renamed, ref current, version, until) = f.retired;
                    let item = RetiredItem::from_number(1 + item % 5)?;
                    let retired = if renamed {
                        RetiredName::renamed(item, current, version.max(1)).until(until)
                    } else {
                        RetiredName::retired(item, version.max(1))
                    };
                    DescribeEntry::retired(&f.name, retired)
                }
                _ => DescribeEntry::new(kind, &f.name, f.bytes.clone()),
            })
        })
        .collect();
    entries.sort_by(|a, b| (a.kind().number(), a.name().as_bytes()).cmp(&(b.kind().number(), b.name().as_bytes())));
    entries.dedup_by(|a, b| (a.kind(), a.name()) == (b.kind(), b.name()));
    entries
}

/// The frames `frames` cut an answer into within `max_frame`: each within it, the last alone flagged `LAST`, each taken
/// by `push`.
fn continues<E: Debug>(frames: &[Vec<u8>], max_frame: usize, mut push: impl FnMut(&FrameView<'_>) -> Result<(), E>) {
    assert!(!frames.is_empty(), "an answer takes a frame");
    for (i, frame) in frames.iter().enumerate() {
        assert!(frame.len() <= max_frame, "a frame within max_frame");
        let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE).expect("a continuation frame validates");
        assert_eq!(
            view.header().flags() & FLAG_LAST != 0,
            i + 1 == frames.len(),
            "the last frame alone is flagged LAST"
        );
        push(&view).expect("a continuation frame is taken");
    }
}

/// The term a query cell's value `n` picks: an IRI, a blank node, a bfloat16vector of 1 to 3 elements, a measure in
/// metres or a precision (`PROTOCOL.md` sections 10.1, 10.5); `None` for an encoding refused.
fn stage_6_term(n: u16) -> Option<ResultTerm> {
    Some(match n % 5 {
        0 => ResultTerm::iri(format!("http://ex.org/{n}")),
        1 => ResultTerm::blank_node(format!(
            "http://ex.org/.well-known/genid/{n:06x}-0123456789abcdef0123456789"
        )),
        2 => {
            let dimension = 1 + n % 3;
            let mut bytes = vec![0x41];
            bytes.extend_from_slice(&dimension.to_le_bytes());
            bytes.extend((0..2 * dimension).map(|i| i as u8));
            ResultTerm::new(EncodedTerm::from_bytes(&bytes).ok()?)
        }
        3 => {
            let mut bytes = vec![0x50];
            bytes.extend_from_slice(&f64::from(n).to_be_bytes());
            bytes.extend_from_slice(&[1, b'm']);
            ResultTerm::new(EncodedTerm::from_bytes(&bytes).ok()?)
        }
        _ => ResultTerm::new(EncodedTerm::from_bytes(&[0x51, 1 + (n % 12) as u8]).ok()?),
    })
}

/// `session` and `request` as a command header takes them: request 0 with no session, at least 1 with one.
fn session_of(session: [u8; 16], request: u64) -> ([u8; 16], u64) {
    (
        session,
        if session == [0; 16] {
            0
        } else {
            request.max(1)
        },
    )
}

/// The Scan the fields make: the choice's version and epoch, a deadline of at least 1, each range's partition at least 1
/// and its keys ordered, a transaction's `at` 1, epoch 0 and deadline 0, its ranges of one partition and its table of the
/// IRIs, at most one per range. `None` with no range left.
fn scan(fields: &ScanFields) -> Option<Scan> {
    let &ScanFields {
        at,
        read_version,
        epoch,
        principal,
        transaction,
        limit,
        deadline_ms,
        ref ranges,
        ref iris,
    } = fields;
    let transaction = transaction.unwrap_or([0; 16]);
    let named = transaction != [0; 16];
    let at = if named {
        ScanAt::ReadVersion
    } else {
        ScanAt::from_number(1 + at % 4)?
    };
    let read_version = if matches!(at, ScanAt::ReadVersion | ScanAt::Named) {
        // A version: its sequence even.
        read_version & !1
    } else {
        0
    };
    let epoch = if at == ScanAt::Named || named {
        0
    } else {
        epoch % (MAX_EPOCH + 1)
    };
    let deadline_ms = if epoch == 0 { 0 } else { deadline_ms };
    let kept: Vec<_> = ranges
        .iter()
        .filter(|range| {
            !range.first.is_empty() && (range.past.is_empty() || range.past > range.first)
        })
        .collect();
    let first_partition = kept.first()?.partition.max(1);
    let mut table = TermTable::new();
    if named {
        iris.iter().take(kept.len()).for_each(|iri| {
            let _ = table.intern(RequestTerm::Iri(iri.clone()));
        });
    }
    let mut built = Scan::new(
        at,
        read_version,
        epoch,
        principal,
        transaction,
        limit,
        deadline_ms,
    )
    .with_terms(table);
    for RangeFields {
        partition,
        first,
        past,
        after,
    } in kept
    {
        let partition = if named {
            first_partition
        } else {
            (*partition).max(1)
        };
        built.push_range(ScanRange::new(partition, first, past).with_after(after));
    }
    Some(built)
}

/// The Entries the fields make: their keys ascending and each once, a batch that continues having `end` 0 and an entry, a
/// run ending every range, its `version` a version.
fn entries_of(
    (range, run): (u32, u16),
    end: u8,
    continues: bool,
    (epoch, version): (u64, u64),
    entries: &[(Vec<u8>, Vec<u8>)],
) -> Option<Entries> {
    if continues && entries.is_empty() {
        return None;
    }
    let epoch = epoch % (MAX_EPOCH + 1);
    let mut built = if continues {
        Entries::continuing(range, epoch)
    } else {
        let end = if run == 0 {
            EntriesEnd::from_number(end % 3)?
        } else {
            EntriesEnd::Ended
        };
        Entries::new(range, end, epoch).with_through(range.checked_add(run.into())?)
    }
    .with_version(version & !1);
    let mut sorted: Vec<_> = entries.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted.dedup_by(|a, b| a.0 == b.0);
    sorted
        .iter()
        .for_each(|(key, value)| built.push(key, value));
    Some(built)
}

/// `body`, read from the frame `accepted` its view accepted, builds that frame, byte for byte, which `read` takes back to
/// the same body.
fn reads_back<B: BodyBuilder + PartialEq + Debug>(
    body: B,
    accepted: &[u8],
    read: impl Fn(&FrameView<'_>) -> Result<B, ProtocolError>,
) {
    let Ok(bytes) = body.frame(Header::new(CLUSTER, Command::Abort)) else {
        return;
    };
    assert_eq!(bytes, accepted, "{:?} has one encoding", B::COMMAND);
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).expect("a built frame");
    assert_eq!(read(&frame).expect("a built body reads"), body);
}

/// The number of sections of `command`'s body, for a command whose body is its sections alone.
fn sections_of(command: Command) -> Option<usize> {
    match command {
        Command::DirectWrite => Some(6),
        Command::ConditionalWrite => Some(7),
        Command::Scan => Some(4),
        Command::Entries => Some(1),
        _ => None,
    }
}

/// `body`'s `n` sections, `None` when its section table does not hold them.
fn split_sections(body: &[u8], n: usize) -> Option<Vec<Vec<u8>>> {
    let u32_at = |at: usize| -> Option<usize> {
        Some(u32::from_le_bytes(body.get(at..at + 4)?.try_into().ok()?) as usize)
    };
    (0..n)
        .map(|i| {
            let (offset, len) = (u32_at(8 * i)?, u32_at(8 * i + 4)?);
            Some(body.get(offset..offset.checked_add(len)?)?.to_vec())
        })
        .collect()
}

/// The body of `sections`: the table of each one's offset and length, each at the end of the one before rounded up to 8.
fn join_sections(sections: &[Vec<u8>]) -> Vec<u8> {
    let mut body = vec![0; 8 * sections.len()];
    for (i, section) in sections.iter().enumerate() {
        body.resize(body.len().next_multiple_of(8), 0);
        let (offset, len) = (body.len() as u32, section.len() as u32);
        body[8 * i..8 * i + 4].copy_from_slice(&offset.to_le_bytes());
        body[8 * i + 4..8 * i + 8].copy_from_slice(&len.to_le_bytes());
        body.extend_from_slice(section);
    }
    body
}

/// `body` of `command` with section `index` changed; as it was for a command or a table this cannot lay out.
fn change_section(body: &mut Vec<u8>, command: Command, index: u8, change: &SectionChange) {
    let Some(mut sections) = sections_of(command).and_then(|n| split_sections(body, n)) else {
        return;
    };
    let count = sections.len();
    let section = &mut sections[usize::from(index) % count];
    match change {
        SectionChange::Clear => section.clear(),
        SectionChange::Truncate { len } => section.truncate(usize::from(*len)),
        SectionChange::Append { byte } => section.push(*byte),
        SectionChange::Set { at, value } => {
            if let Some(byte) = section.get_mut(usize::from(*at)) {
                *byte = *value;
            }
        }
        SectionChange::Absent { at } => {
            let start = usize::from(*at).checked_rem(section.len()).unwrap_or(0);
            if let Some(slot) = section.get_mut(start..start + 4) {
                slot.fill(0xFF);
            }
        }
    }
    *body = join_sections(&sections);
}

/// A check that a built frame's view reads back what was built.
type Check = Box<dyn Fn(&FrameView<'_>)>;

/// The frame of `body`, and a check that its view reads back what was built, or `None` when the fields make no
/// valid body.
fn build(body: &Body, terms: TermTable, header: Header) -> Option<(Vec<u8>, Check)> {
    let n = terms.len() as u32;
    let t = |i: u16| u32::from(i) % n;
    Some(match body {
        Body::Hello {
            min,
            extra,
            release,
            max_frame,
            name,
        } => {
            let min = (*min).max(1);
            let floor = MAX_FRAME_FLOOR as u32;
            let hello = Hello::new(
                min,
                min.saturating_add(u16::from(*extra)),
                *release,
                floor + max_frame % (MAX_FRAME_SIZE as u32 - floor + 1),
                name.clone(),
            );
            let frame = hello.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(HelloView::from_frame(f).unwrap().to_body().unwrap(), hello)
                }),
            )
        }
        Body::Welcome {
            protocol,
            max_frame,
            streams,
            credit,
            write_set,
            transaction_ms,
            name,
            mechanisms,
        } => {
            let floor = MAX_FRAME_FLOOR as u32;
            let welcome = Welcome::new((*protocol).max(1), 0, name.clone())
                .with_max_frame(floor + max_frame % (MAX_FRAME_SIZE as u32 - floor + 1))
                .with_max_streams((*streams).max(1))
                .with_initial_credit((*credit).max(1))
                .with_max_write_set(*write_set)
                .with_transaction_ms(*transaction_ms)
                .with_mechanisms(mechanisms.iter().filter(|m| !m.is_empty()).cloned());
            let frame = welcome.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        WelcomeView::from_frame(f).unwrap().to_body().unwrap(),
                        welcome
                    )
                }),
            )
        }
        Body::Commit {
            session,
            request,
            version,
            results,
        } => {
            // A request names commit version 0, no token and no results; a reply a version of an epoch 1 to
            // 2^42 - 1, that epoch's token and its results, each node by its IRI in the term table (PROTOCOL.md 6.3).
            let (session, request) = session_of(*session, *request);
            let mut commit = Commit::new(session, request, *version);
            if *version != 0 {
                // A reply through no session holds the zero token.
                if session != [0; 16] {
                    commit = commit.with_token(token_of(*version, request)?);
                }
                for (u, c) in results {
                    if *c {
                        commit.push_node_result(&format!("http://ex.org/n{u}"), true);
                    } else {
                        commit.push_result(OperationResult::counter(*u as i64));
                    }
                }
            }
            let frame = commit.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        CommitView::from_frame(f).unwrap().to_body().unwrap(),
                        commit
                    )
                }),
            )
        }
        Body::Credit { frames } => {
            let credit = Credit::new((*frames).max(1));
            let frame = credit.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        CreditView::from_frame(f).unwrap().to_body().unwrap(),
                        credit
                    )
                }),
            )
        }
        Body::Error {
            code,
            detail,
            message,
            value,
        } => {
            let code = ErrorCode::ALL[usize::from(*code) % ErrorCode::ALL.len()];
            let mut reply = ErrorReply::new(code, message.clone()).with_detail(*detail);
            if let Some(value) = value {
                reply = reply.with_value(RequestTerm::Iri(value.clone()));
            }
            let frame = reply.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        ErrorReplyView::from_frame(f).unwrap().to_body().unwrap(),
                        reply
                    )
                }),
            )
        }
        Body::Read {
            transaction,
            limit,
            slots,
            resume,
        } => {
            let mut parameters = 0;
            let s: Vec<_> = slots.iter().map(|s| slot(s, &mut parameters, n)).collect();
            if parameters > 0 {
                return None;
            }
            let read = Read::new(
                *transaction,
                *limit,
                terms,
                QuadPattern::new(s[0], s[1], s[2], s[3]),
                resume.clone(),
            );
            let frame = read.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(ReadView::from_frame(f).unwrap().to_body().unwrap(), read)
                }),
            )
        }
        Body::Prepare {
            slots,
            node_parameters,
        } => {
            let mut parameters = 0;
            let s: Vec<_> = slots.iter().map(|s| slot(s, &mut parameters, n)).collect();
            let kind = if *node_parameters {
                ColumnType::Node
            } else {
                ColumnType::Term
            };
            let prepare = Prepare::new(
                0,
                0,
                0,
                terms,
                QuadPattern::new(s[0], s[1], s[2], s[3]),
                vec![kind; parameters as usize],
                vec![],
            );
            let frame = prepare.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        PrepareView::from_frame(f).unwrap().to_body().unwrap(),
                        prepare
                    )
                }),
            )
        }
        Body::Execute {
            statement,
            transaction,
            parameters,
            resume,
        } => {
            let parameters = parameters.iter().map(|p| t(*p)).collect();
            let execute = Execute::new(
                *statement,
                *transaction,
                0,
                terms,
                parameters,
                resume.clone(),
            );
            let frame = execute.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        ExecuteView::from_frame(f).unwrap().to_body().unwrap(),
                        execute
                    )
                }),
            )
        }
        Body::Write { changes } => {
            let changes = changes
                .iter()
                .map(|(kind, s, p, o)| {
                    let graph = (kind % 2 == 1).then(|| t(*o));
                    match kind % 3 {
                        0 => WriteChange::Insert {
                            graph,
                            subject: t(*s),
                            predicate: t(*p),
                            object: t(*o),
                        },
                        1 => WriteChange::Delete {
                            graph,
                            subject: t(*s),
                            predicate: t(*p),
                            object: t(*o),
                        },
                        _ => WriteChange::DeletePattern {
                            graph,
                            subject: t(*s),
                            predicate: t(*p),
                        },
                    }
                })
                .collect();
            let write = Write::new(terms, changes);
            let frame = write.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(WriteView::from_frame(f).unwrap().to_body().unwrap(), write)
                }),
            )
        }
        Body::Operation(fields) => {
            let op = Operation::new(terms, commit_operation(fields, &t));
            let frame = op.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(OperationView::from_frame(f).unwrap().to_body().unwrap(), op)
                }),
            )
        }
        Body::Result { columns, rows } => {
            // The unbound slots and the reifier column: 1 to 5 columns (PROTOCOL.md 9.1).
            let columns = 1 + u32::from(columns % 5);
            let mut batch = ResultBatch::new(columns);
            for row in rows {
                let cells: Vec<Option<ResultTerm>> = (0..columns as usize)
                    .map(|c| {
                        row.get(c).copied().flatten().map(|n| {
                            if n % 2 == 1 {
                                ResultTerm::blank_node(format!(
                                    "http://ex.org/.well-known/genid/{n:06x}-0123456789abcdef0123456789"
                                ))
                            } else {
                                ResultTerm::iri(format!("http://ex.org/{n}"))
                            }
                        })
                    })
                    .collect();
                let refs: Vec<Option<&ResultTerm>> = cells.iter().map(Option::as_ref).collect();
                batch.push_row(&refs).ok()?;
            }
            let frame = batch.frame(header).ok()?;
            let read_back = batch;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        ResultView::from_frame(f).unwrap().to_body().unwrap(),
                        read_back
                    )
                }),
            )
        }
        Body::Authenticate {
            step,
            mechanism,
            message,
            last,
        } => {
            // A client's message is even and alone names the mechanism, at step 0; a server's is odd and may be final.
            let step = u32::from(*step);
            let mut authenticate = match last {
                Some(_) => Authenticate::new(step | 1, "", message.clone()),
                None if step == 0 => {
                    let name = if mechanism.is_empty() {
                        "SCRAM-SHA-256"
                    } else {
                        mechanism
                    };
                    Authenticate::new(0, name, message.clone())
                }
                None => Authenticate::new(step, "", message.clone()),
            };
            if let Some((principal, expires)) = last {
                authenticate = authenticate.with_final(*principal, *expires);
            }
            let frame = authenticate.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        AuthenticateView::from_frame(f).unwrap().to_body().unwrap(),
                        authenticate
                    )
                }),
            )
        }
        Body::Drop {
            kind,
            tenant,
            target,
            session,
            request,
            version,
            catalog,
        } => {
            let (session, request) = session_of(*session, *request);
            let command = match kind % 4 {
                0 => DropCommand::predicate(*tenant, target.clone()),
                1 => DropCommand::graph(*tenant, target.clone()),
                2 => DropCommand::tenant_data(*tenant).with_catalog(*catalog),
                _ => DropCommand::whole_tenant(*tenant).with_catalog(*catalog),
            }
            .with_session(session, request)
            .with_commit_version(*version);
            // A reply names its commit version and, through a session, that epoch's token (PROTOCOL.md 6.5).
            let command = if *version == 0 || session == [0; 16] {
                command
            } else {
                command.with_token(token_of(*version, request)?)
            };
            let frame = command.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        DropCommandView::from_frame(f).unwrap().to_body().unwrap(),
                        command
                    )
                }),
            )
        }
        Body::Round { decisions, keys } => {
            let mut round = RoundBody::new(3, 4, 1, 100, NonZeroU32::MIN);
            for key in keys {
                round.insert_dictionary(key.clone(), key.clone());
            }
            for (i, decision) in decisions.iter().enumerate() {
                match decision {
                    None => {
                        let mut record = WriteSetRecord::transaction(0);
                        record.set_read_version(99);
                        record.set_session([i as u8; 16], 1);
                        for key in keys {
                            record.push_inserted(key.clone(), None);
                            record.push_conflict_point(key.clone());
                        }
                        round.commit(record.encode());
                    }
                    Some((code, detail)) => {
                        let code = ErrorCode::ALL[usize::from(*code) % ErrorCode::ALL.len()];
                        round.abort(code, *detail);
                    }
                }
            }
            let frame = round.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(RoundView::from_frame(f).unwrap().to_body().unwrap(), round)
                }),
            )
        }
        Body::DirectWrite {
            session,
            request,
            tenant,
            token,
            changes,
        } => {
            // A DirectWrite holds at least one change (PROTOCOL.md 6.6).
            if changes.is_empty() {
                return None;
            }
            let changes = changes.iter().map(|fields| change(fields, &t)).collect();
            let (session, request) = session_of(*session, *request);
            let mut write =
                DirectWrite::new(*tenant, terms, changes).with_session(session, request);
            if let Some(token) = token.and_then(|version| token_of(version, request)) {
                write = write.with_token(token);
            }
            let frame = write.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        DirectWriteView::from_frame(f).unwrap().to_body().unwrap(),
                        write
                    )
                }),
            )
        }
        Body::ConditionalWrite {
            session,
            request,
            tenant,
            token,
            changes,
            operations,
        } => {
            // A ConditionalWrite holds at least one operation (PROTOCOL.md 6.7).
            if operations.is_empty() {
                return None;
            }
            let changes = changes.iter().map(|fields| change(fields, &t)).collect();
            let operations = operations
                .iter()
                .map(|fields| commit_operation(fields, &t))
                .collect();
            let (session, request) = session_of(*session, *request);
            let mut write = ConditionalWrite::new(*tenant, terms, changes, operations)
                .with_session(session, request);
            if let Some(token) = token.and_then(|version| token_of(version, request)) {
                write = write.with_token(token);
            }
            let frame = write.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        ConditionalWriteView::from_frame(f)
                            .unwrap()
                            .to_body()
                            .unwrap(),
                        write
                    )
                }),
            )
        }
        Body::Scan(fields) => {
            let built = scan(fields)?;
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(ScanView::from_frame(f).unwrap().to_body().unwrap(), built)
                }),
            )
        }
        Body::Entries {
            range,
            run,
            end,
            continues,
            epoch,
            version,
            entries,
        } => {
            let built = entries_of(
                (*range, *run),
                *end,
                *continues,
                (*epoch, *version),
                entries,
            )?;
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        EntriesView::from_frame(f).unwrap().to_body().unwrap(),
                        built
                    )
                }),
            )
        }
        Body::Query {
            transaction,
            language,
            limit,
            text,
            parameters,
            resume,
        } => {
            let language = Language::from_number(1 + language % 3)?;
            let built = parameters.iter().fold(
                Query::new(*transaction, language, text.clone(), terms)
                    .with_limit(*limit)
                    .with_resume(resume.clone()),
                |query, (name, value)| query.with_parameter(name.clone(), u32::from(*value) % n),
            );
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(QueryView::from_frame(f).unwrap().to_body().unwrap(), built)
                }),
            )
        }
        Body::Install {
            tenant,
            version,
            sources,
            answered,
            staged,
            warnings: warning_fields,
        } => {
            // At least one source, each name within its bound.
            if sources.is_empty() || sources.iter().any(|(name, _)| name.len() > 255) {
                return None;
            }
            let request = Install::new(*tenant, *version, sources.clone());
            let built = match answered {
                Some((installed, epoch, report)) => {
                    let reply = request
                        .installed((*installed).max(1), 1 + epoch % MAX_EPOCH, report.clone())
                        .with_warnings(warnings(warning_fields, true));
                    // A migration report comes with a staged namespace alone.
                    match staged {
                        (0, _) => reply,
                        (staged, report) => reply.with_staged(*staged).with_migration_report(report.clone()),
                    }
                }
                None => request,
            };
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        InstallView::from_frame(f).unwrap().to_body().unwrap(),
                        built
                    )
                }),
            )
        }
        Body::PrepareText {
            language,
            text,
            parameters,
            answered,
            warnings: warning_fields,
        } => {
            let language = Language::from_number(1 + language % 3)?;
            let column = |byte: u8| ColumnType::from_number(1 + byte % 13);
            let names = parameters.iter().map(|(name, _)| name.clone()).collect();
            let types = parameters.iter().map(|(_, t)| column(*t)).collect::<Option<_>>()?;
            let request = Prepare::of_text(0, language, text.clone(), terms, names, types);
            // A reply names its statement, at least 1, and alone carries warnings (PROTOCOL.md 8.1).
            let prepare = match answered {
                Some((statement, schema, projections)) => {
                    let projections = projections
                        .iter()
                        .map(|(name, t)| Some((name.clone(), column(*t)?)))
                        .collect::<Option<_>>()?;
                    request
                        .answered((*statement).max(1), *schema, projections)
                        .with_warnings(warnings(warning_fields, false))
                }
                None => request,
            };
            let frame = prepare.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        PrepareView::from_frame(f).unwrap().to_body().unwrap(),
                        prepare
                    )
                }),
            )
        }
        Body::QueryResult {
            names,
            rows,
            warnings: warning_fields,
        } => {
            // One to four projections, each named (PROTOCOL.md 9.1), each cell's term as `stage_6_term` picks it.
            let columns = 1 + names.len() % 4;
            let column_names = (0..columns)
                .map(|c| names.get(c).cloned().unwrap_or_default())
                .collect();
            let mut batch = ResultBatch::new(columns as u32)
                .with_column_names(column_names)
                .with_warnings(warnings(warning_fields, false));
            for row in rows {
                // A cell names its term, or none for null; a term its encoding refuses ends the input.
                let cells = (0..columns)
                    .map(|c| match row.get(c).copied().flatten() {
                        None => Some(None),
                        Some(n) => stage_6_term(n).map(Some),
                    })
                    .collect::<Option<Vec<Option<ResultTerm>>>>()?;
                let refs: Vec<Option<&ResultTerm>> = cells.iter().map(Option::as_ref).collect();
                batch.push_query_row(&refs).ok()?;
            }
            let frame = batch.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        ResultView::from_frame(f).unwrap().to_body().unwrap(),
                        batch
                    )
                }),
            )
        }
        Body::Describe {
            tenant,
            read_version,
            entries,
            max_frame,
        } => {
            let built = Describe::new(*tenant, *read_version).with_entries(describe_entries(entries));
            let max_frame = 1024 + usize::from(*max_frame);
            if let Ok(frames) = built.frames(header.clone(), max_frame) {
                let mut answer = DescribeFrames::new();
                continues(&frames, max_frame, |f| answer.push(f));
                assert!(answer.is_last());
                assert_eq!(
                    answer.view().unwrap().to_body().unwrap(),
                    built,
                    "a Describe's frames take it back"
                );
            }
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(
                        DescribeView::from_frame(f).unwrap().to_body().unwrap(),
                        built
                    )
                }),
            )
        }
        Body::Purge {
            tenant,
            iris,
            purged,
        } => {
            // 1 to 10,000 IRIs, each once; a reply's epoch and at most as many removed (PROTOCOL.md 8.5).
            let mut unique: Vec<String> = Vec::new();
            for iri in iris.iter().take(PURGE_IRIS_MAX) {
                if !unique.contains(iri) {
                    unique.push(iri.clone());
                }
            }
            if unique.is_empty() {
                return None;
            }
            let count = unique.len() as u32;
            let request = Purge::new(*tenant, unique);
            let built = match purged {
                Some((epoch, removed)) => request.purged(1 + epoch % MAX_EPOCH, removed % (count + 1)),
                None => request,
            };
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(PurgeView::from_frame(f).unwrap().to_body().unwrap(), built)
                }),
            )
        }
        Body::Migrate {
            tenant,
            staged,
            action,
            answered,
        } => {
            let request = Migrate::new(
                *tenant,
                (*staged).max(1),
                MigrateAction::from_number(1 + action % 5)?,
            );
            // A reply's state 1 to 9; a refusal, UGS2201 to UGS2299, with its message for a failed run alone
            // (PROTOCOL.md 8.6).
            let built = match answered {
                Some((state, epoch, report, refusal)) => {
                    let state = MigrateState::from_number(1 + state % 9)?;
                    let reply = request.answered(state, epoch % (MAX_EPOCH + 1), report.clone());
                    match refusal {
                        Some((number, message)) if state == MigrateState::Failed => {
                            reply.refused(2_201 + u32::from(number % 99), message.clone())
                        }
                        _ => reply,
                    }
                }
                None => request,
            };
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(MigrateView::from_frame(f).unwrap().to_body().unwrap(), built)
                }),
            )
        }
        Body::Catalog {
            tenant,
            version,
            answered,
            max_frame,
        } => {
            // A request names the version, 0 for the current one; a reply a version of at least 1, its epoch, its
            // position, at least 1, and its bytes (PROTOCOL.md 13.7).
            let built = match answered {
                Some((epoch, position, bytes)) => Catalog::reply(
                    *tenant,
                    (*version).max(1),
                    epoch % (MAX_EPOCH + 1),
                    (*position).max(1),
                    bytes.clone(),
                ),
                None => Catalog::request(*tenant, *version),
            };
            let max_frame = 1024 + usize::from(*max_frame);
            if let Ok(frames) = built.frames(header.clone(), max_frame) {
                let mut answer = CatalogFrames::new();
                continues(&frames, max_frame, |f| answer.push(f));
                assert!(answer.is_last());
                assert_eq!(
                    answer.view().unwrap().to_body().unwrap(),
                    built,
                    "a Catalog's frames take it back"
                );
            }
            let frame = built.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(CatalogView::from_frame(f).unwrap().to_body().unwrap(), built)
                }),
            )
        }
    })
}

fuzz_target!(|input: Input| {
    let header = Header::new(CLUSTER, Command::Abort).with_stream(input.stream);
    let Some((frame, check)) = build(&input.body, table(&input.terms), header) else {
        return;
    };
    let view =
        FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE).expect("a built frame validates");
    check(&view);
    let command = view.command();
    let mut body = view.body().to_vec();
    let mut command_header = *view.header().command_header();
    for mutation in &input.mutations {
        match mutation {
            Mutation::Flip { at, bit } => {
                if let Some(byte) = body.get_mut(usize::from(*at)) {
                    *byte ^= 1 << (bit % 8);
                }
            }
            Mutation::Truncate { len } => body.truncate(usize::from(*len)),
            Mutation::Append { byte } => body.push(*byte),
            Mutation::Set { at, value } => {
                if let Some(slot) = body.get_mut(usize::from(*at)..usize::from(*at) + 4) {
                    slot.copy_from_slice(&value.to_le_bytes());
                }
            }
            Mutation::CommandHeader { at, value } => {
                command_header[usize::from(*at) % command_header.len()] = *value;
            }
            Mutation::Section { index, change } => {
                change_section(&mut body, command, *index, change);
            }
        }
    }
    let Ok(mutated) = Header::new(CLUSTER, command)
        .with_command_header(command_header)
        .encode_frame(&body)
    else {
        return;
    };
    let frame =
        FrameView::validate(&mutated, CLUSTER, MAX_FRAME_SIZE).expect("a rebuilt frame validates");
    // The mutated body is read or refused by its view; neither panics.
    let _ = match command {
        Command::Hello => HelloView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Welcome => WelcomeView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Commit => CommitView::from_frame(&frame)
            .inspect(|v| {
                assert_eq!(v.session() == [0; 16], v.request() == 0);
            })
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Credit => CreditView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Error => ErrorReplyView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Read => ReadView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Prepare => PrepareView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Execute => ExecuteView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Write => WriteView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Operation => OperationView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Result => ResultView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Round => RoundView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Authenticate => AuthenticateView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Drop => DropCommandView::from_frame(&frame)
            .inspect(|v| {
                assert_eq!(v.session() == [0; 16], v.request() == 0);
            })
            .and_then(|v| v.to_body())
            .map(drop),
        Command::DirectWrite => DirectWriteView::from_frame(&frame)
            .inspect(|v| {
                assert_eq!(v.session() == [0; 16], v.request() == 0);
            })
            .and_then(|v| v.to_body())
            .map(|body| {
                reads_back(body, &mutated, |f| {
                    DirectWriteView::from_frame(f)?.to_body()
                })
            }),
        Command::ConditionalWrite => ConditionalWriteView::from_frame(&frame)
            .inspect(|v| {
                assert_eq!(v.session() == [0; 16], v.request() == 0);
            })
            .and_then(|v| v.to_body())
            .map(|body| {
                reads_back(body, &mutated, |f| {
                    ConditionalWriteView::from_frame(f)?.to_body()
                })
            }),
        Command::Scan => ScanView::from_frame(&frame)
            .inspect(|v| {
                if v.transaction() != [0; 16] {
                    assert_eq!(
                        (v.at(), v.epoch(), v.deadline_ms()),
                        (ScanAt::ReadVersion, 0, 0)
                    );
                    assert!(v.terms().len() <= v.range_count());
                }
                assert!(v.epoch() != 0 || v.deadline_ms() == 0);
            })
            .and_then(|v| v.to_body())
            .map(|body| reads_back(body, &mutated, |f| ScanView::from_frame(f)?.to_body())),
        Command::Entries => EntriesView::from_frame(&frame)
            .inspect(|v| {
                if v.continues() {
                    assert_eq!(v.end(), EntriesEnd::More);
                    assert!(!v.entries().is_empty());
                }
            })
            .and_then(|v| v.to_body())
            .map(|body| reads_back(body, &mutated, |f| EntriesView::from_frame(f)?.to_body())),
        Command::Install => InstallView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Query => QueryView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Describe => DescribeView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Purge => PurgeView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(|body| reads_back(body, &mutated, |f| PurgeView::from_frame(f)?.to_body())),
        Command::Migrate => MigrateView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(|body| reads_back(body, &mutated, |f| MigrateView::from_frame(f)?.to_body())),
        Command::Catalog => CatalogView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(|body| reads_back(body, &mutated, |f| CatalogView::from_frame(f)?.to_body())),
        _ => Ok(()),
    };
});

/// The token a reply at commit `version` carries: its epoch `version >> 22`, which a token holds from 1 to 2^42 - 1,
/// at position `request`, at least 1; `None` for a version of no such epoch.
fn token_of(version: u64, request: u64) -> Option<SessionToken> {
    let epoch = Epoch::new(version >> 22).ok().filter(|e| e.get() != 0)?;
    Some(SessionToken::new(1, request.max(1), epoch))
}
