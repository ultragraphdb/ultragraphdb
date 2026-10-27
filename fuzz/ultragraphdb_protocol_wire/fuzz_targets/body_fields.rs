/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Structure-aware: bodies of every kind built from arbitrary fields, which must validate and read back to what was
//! built; then arbitrary mutations of the body, the frame rebuilt around it so the body's checks are reached, which
//! never panic.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use oxrdf::{GraphName, Literal, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_core_mapping::{Change, Changeset};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, Command, Commit, CommitOperation, CommitView, Credit,
    CreditView, ErrorCode, ErrorReply, ErrorReplyView, Execute, ExecuteView, FrameView, Header,
    Hello, HelloView, Link, Operation, OperationResult, OperationView, PatternSlot, Prepare,
    PrepareView, QuadPattern, Read, ReadView, ResultBatch, ResultTerm, ResultView, RoundBody,
    RoundView, Welcome, WelcomeView, Write, WriteSetRecord, WriteView,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

#[derive(Debug, Arbitrary)]
enum Slot {
    Unbound,
    Bound(u8),
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
        parameters: Vec<String>,
        resume: Vec<u8>,
    },
    Write {
        namespace: u64,
        changes: Vec<(u8, u8, u8, u8)>,
    },
    Operation {
        kind: u8,
        names: [u8; 4],
        delta: i64,
        links: Vec<(u8, bool)>,
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
    body: Body,
    stream: u64,
    mutations: Vec<Mutation>,
}

fn node(n: u8) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/n{n}"))
}

fn term(n: u8) -> Term {
    match n % 3 {
        0 => node(n).into(),
        1 => Literal::new_simple_literal(format!("v{n}")).into(),
        _ => Literal::new_language_tagged_literal_unchecked(format!("w{n}"), "en").into(),
    }
}

fn slot(s: &Slot, parameters: &mut u32) -> PatternSlot<String> {
    match s {
        Slot::Unbound => PatternSlot::Unbound,
        Slot::Bound(n) => PatternSlot::Bound(node(*n).to_string()),
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
fn build(body: &Body, header: Header) -> Option<(Vec<u8>, Check)> {
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
            let s: Vec<_> = slots.iter().map(|s| slot(s, &mut parameters)).collect();
            if parameters > 0 {
                return None;
            }
            let [g, su, p, o]: [PatternSlot<String>; 4] = s.try_into().ok()?;
            let read = Read::new(
                *namespace,
                *limit,
                QuadPattern::new(g, su, p, o),
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
            let s: Vec<_> = slots.iter().map(|s| slot(s, &mut parameters)).collect();
            let [g, su, p, o]: [PatternSlot<String>; 4] = s.try_into().ok()?;
            let kind = if *node_parameters {
                ColumnType::Node
            } else {
                ColumnType::Term
            };
            let prepare = Prepare::new(
                0,
                0,
                0,
                QuadPattern::new(g, su, p, o),
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
            let execute = Execute::new(*statement, 0, parameters.clone(), resume.clone());
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
            let mut changeset = Changeset::new(*namespace);
            for (kind, s, p, o) in changes {
                let subject: NamedOrBlankNode = node(*s).into();
                let graph = if kind % 2 == 0 {
                    GraphName::DefaultGraph
                } else {
                    node(*o).into()
                };
                changeset.push(match kind % 3 {
                    0 => Change::Insert {
                        graph,
                        subject,
                        predicate: node(*p),
                        object: term(*o),
                    },
                    1 => Change::Delete {
                        graph,
                        subject,
                        predicate: node(*p),
                        object: term(*o),
                    },
                    _ => Change::DeletePattern {
                        graph,
                        subject,
                        predicate: node(*p),
                    },
                });
            }
            let write = Write::new(changeset);
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
            names: [a, b, c, d],
            delta,
            links,
        } => {
            let operation = match kind % 4 {
                0 => CommitOperation::InsertIfAbsent {
                    graph: GraphName::DefaultGraph,
                    predicate: node(*a),
                    value: term(*b),
                    properties: vec![(node(*c), term(*d))],
                },
                1 => CommitOperation::UpsertAndLink {
                    graph: GraphName::DefaultGraph,
                    predicate: node(*a),
                    value: term(*b),
                    properties: vec![],
                    links: links
                        .iter()
                        .map(|(n, from)| {
                            if *from {
                                Link::from_node(node(*n), node(*n).into())
                            } else {
                                Link::to_node(node(*n), node(*n).into())
                            }
                        })
                        .collect(),
                },
                2 => CommitOperation::CompareAndSet {
                    graph: node(*a).into(),
                    subject: node(*b).into(),
                    predicate: node(*c),
                    expected: term(*d),
                    new: term(*a),
                },
                _ => CommitOperation::CounterAdd {
                    graph: GraphName::DefaultGraph,
                    subject: node(*a).into(),
                    predicate: node(*b),
                    delta: *delta,
                },
            };
            let op = Operation::new(operation);
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
                let terms: Vec<Option<ResultTerm>> = (0..columns as usize)
                    .map(|c| {
                        row.get(c).copied().flatten().map(|n| {
                            ResultTerm::new(EncodedTerm::uid(u64::from(n)))
                                .with_text(format!("http://ex.org/{n}"))
                        })
                    })
                    .collect();
                let refs: Vec<Option<&ResultTerm>> = terms.iter().map(Option::as_ref).collect();
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
    let Some((frame, check)) = build(&input.body, header) else {
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
