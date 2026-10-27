/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Structure-aware: bodies of every kind, their term tables included, built from arbitrary fields, which must validate
//! and read back to what was built; then arbitrary mutations of the body, the frame rebuilt around it so the body's checks are reached, which
//! never panic.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, Commit, CommitOperation, CommitView, Credit,
    CreditView, Direction, ErrorCode, ErrorReply, ErrorReplyView, Execute, ExecuteView, FrameView,
    Header, Hello, HelloView, Link, Operation, OperationResult, OperationView, PatternSlot,
    Prepare, PrepareView, QuadPattern, Read, ReadView, RequestTerm, ResultBatch, ResultTerm,
    ResultView, RoundBody, RoundView, TermTable, Welcome, WelcomeView, Write, WriteChange,
    WriteSetRecord, WriteView,
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

#[derive(Debug, Arbitrary)]
enum Body {
    Hello {
        min: u16,
        extra: u8,
        release: u32,
        name: String,
    },
    Welcome {
        protocol: u16,
        max_frame: u32,
        streams: u32,
        credit: u32,
        name: String,
    },
    Commit {
        session: u64,
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
        namespace: u64,
        limit: u32,
        slots: [Slot; 4],
        resume: Vec<u8>,
    },
    Prepare {
        slots: [Slot; 4],
        node_parameters: bool,
    },
    Execute {
        statement: u64,
        parameters: Vec<u16>,
        resume: Vec<u8>,
    },
    Write {
        namespace: u64,
        changes: Vec<(u8, u16, u16, u16)>,
    },
    Operation {
        kind: u8,
        args: [u16; 4],
        delta: i64,
        links: Vec<(u16, bool)>,
    },
    Result {
        columns: u8,
        rows: Vec<Vec<Option<u16>>>,
    },
    Round {
        decisions: Vec<Option<(u8, u64)>>,
        keys: Vec<Vec<u8>>,
    },
}

#[derive(Debug, Arbitrary)]
enum Mutation {
    Flip { at: u16, bit: u8 },
    Truncate { len: u16 },
    Append { byte: u8 },
    Set { at: u16, value: u32 },
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
            name,
        } => {
            let min = (*min).max(1);
            let hello = Hello::new(
                min,
                min.saturating_add(u16::from(*extra)),
                *release,
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
            name,
        } => {
            let max_frame = 4_096 + max_frame % (1_048_576 - 4_096 + 1);
            let welcome = Welcome::new(
                (*protocol).max(1),
                0,
                max_frame,
                (*streams).max(1),
                *credit,
                name.clone(),
            );
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
            let results = results
                .iter()
                .map(|(u, c)| OperationResult::new(*u, *c))
                .collect();
            let commit = Commit::new(*session, *request, *version, results);
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
            if let Some(value) = value.as_ref().filter(|v| !v.is_empty()) {
                reply = reply.with_value(value.clone());
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
            namespace,
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
                *namespace,
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
            parameters,
            resume,
        } => {
            let parameters = parameters.iter().map(|p| t(*p)).collect();
            let execute = Execute::new(*statement, 0, terms, parameters, resume.clone());
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
        Body::Write { namespace, changes } => {
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
            let write = Write::new(*namespace, terms, changes);
            let frame = write.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(WriteView::from_frame(f).unwrap().to_body().unwrap(), write)
                }),
            )
        }
        Body::Operation {
            kind,
            args: [a, b, c, d],
            delta,
            links,
        } => {
            let operation = match kind % 4 {
                0 => CommitOperation::InsertIfAbsent {
                    graph: None,
                    predicate: t(*a),
                    value: t(*b),
                    properties: vec![(t(*c), t(*d))],
                },
                1 => CommitOperation::UpsertAndLink {
                    graph: None,
                    predicate: t(*a),
                    value: t(*b),
                    properties: vec![],
                    links: links
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
                    graph: Some(t(*a)),
                    subject: t(*b),
                    predicate: t(*c),
                    expected: t(*d),
                    new: t(*a),
                },
                _ => CommitOperation::CounterAdd {
                    graph: None,
                    subject: t(*a),
                    predicate: t(*b),
                    delta: *delta,
                },
            };
            let op = Operation::new(terms, operation);
            let frame = op.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(OperationView::from_frame(f).unwrap().to_body().unwrap(), op)
                }),
            )
        }
        Body::Result { columns, rows } => {
            let columns = u32::from(columns % 8);
            let mut batch = ResultBatch::new(columns);
            for row in rows {
                let cells: Vec<Option<ResultTerm>> = (0..columns as usize)
                    .map(|c| {
                        row.get(c).copied().flatten().map(|n| {
                            ResultTerm::new(EncodedTerm::uid(u64::from(n)))
                                .with_text(format!("http://ex.org/{n}"))
                        })
                    })
                    .collect();
                let refs: Vec<Option<&ResultTerm>> = cells.iter().map(Option::as_ref).collect();
                batch.push_row(&refs).ok()?;
            }
            let frame = batch.frame(header).ok()?;
            (
                frame,
                Box::new(move |f| {
                    assert_eq!(ResultView::from_frame(f).unwrap().to_body().unwrap(), batch)
                }),
            )
        }
        Body::Round { decisions, keys } => {
            let mut round = RoundBody::new(3, 4, 1, 100);
            for (i, decision) in decisions.iter().enumerate() {
                match decision {
                    None => {
                        let mut record = WriteSetRecord::new(0, 99, i as u64, 1);
                        for key in keys {
                            record.push_list_delta(key.clone(), key.clone());
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
    })
}

fuzz_target!(|input: Input| {
    let header = Header::new(CLUSTER, Command::Abort).with_stream(input.stream);
    let Some((frame, check)) = build(&input.body, table(&input.terms), header) else {
        return;
    };
    let view = FrameView::validate(&frame, CLUSTER).expect("a built frame validates");
    check(&view);
    let command = view.command();
    let mut body = view.body().to_vec();
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
        }
    }
    let Ok(mutated) = Header::new(CLUSTER, command)
        .with_command_header(*view.header().command_header())
        .encode_frame(&body)
    else {
        return;
    };
    let frame = FrameView::validate(&mutated, CLUSTER).expect("a rebuilt frame validates");
    // The mutated body is read or refused by its view; neither panics.
    let _ = match command {
        Command::Hello => HelloView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Welcome => WelcomeView::from_frame(&frame)
            .and_then(|v| v.to_body())
            .map(drop),
        Command::Commit => CommitView::from_frame(&frame)
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
        _ => Ok(()),
    };
});
