/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The reads a partition's core answers (`PROTOCOL.md` §§13.13, 13.14): Scans, Closed and Recall, against a core of
//! `core_harness`, its frames read through a session or straight from the core.

use ultragraphdb_cluster_partition::PartitionMessage;
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_common_role::{Answer, RoleClient};
use ultragraphdb_core_ids::{Epoch, SystemId};
use ultragraphdb_core_keys::{Graph, Keyspace, QuadKey, key_prefix};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BindView, BodyView, ClosedView, Command, DirectWrite, EntriesEnd, EntriesView,
    ErrorCode, ErrorReplyView, FLAG_LAST, MAX_FRAME_SIZE, ObjectRecords, RequestTerm, Scan, ScanAt,
    ScanRange, TermTable, WriteChange,
};
use ultragraphdb_store_quads::{StatementRecord, term_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::types::partition_core::core_rig::{PARTITION, PRINCIPAL, RigSetup, TENANT, with_core};
use crate::types::partition_core::core_writes::{
    Direct, alone, edges, ex, limited, ms, one_frame, plain, refusal, scanned, viewed,
};

/// The subjects of the large writes: lists enough for many frames of [`SMALL`] bytes.
const SUBJECTS: usize = 400;
/// The Direct connection's number, beside the sessions the harness numbers from 1.
const DIRECT: u64 = 1_000;
/// The frames a reader taking any frame takes.
const LARGE: u32 = MAX_FRAME_SIZE as u32;
/// The frames of a reader whose small frames make a range of [`SUBJECTS`] lists take many.
const SMALL: u32 = 4 * 1_024;

/// A DirectWrite giving each of [`SUBJECTS`] subjects an object.
fn large() -> DirectWrite {
    let mut terms = TermTable::new();
    let (predicate, object) = (terms.iri(&ex("p")), terms.iri(&ex("o")));
    let changes = (0..SUBJECTS)
        .map(|at| WriteChange::Insert {
            graph: None,
            subject: terms.iri(&ex(&format!("s{at}"))),
            predicate,
            object,
        })
        .collect();
    DirectWrite::new(TENANT, terms, changes).with_principal(PRINCIPAL)
}

/// Commits `write` through a session of `client`, failing on a refusal.
async fn commit(client: &mut RoleClient, write: &DirectWrite) -> TestResult<u64> {
    let committed = one_frame(client, write)
        .await?
        .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
    Ok(committed.version)
}

/// The tenant's whole forward keyspace, as one range.
fn forward() -> (Vec<u8>, Vec<u8>) {
    (
        key_prefix(Keyspace::Forward, false, TENANT, None, None),
        Vec::new(),
    )
}

/// An Entries frame's range, through, end, epoch, last flag and entries.
struct Pushed {
    range: u32,
    through: u32,
    end: u8,
    epoch: u64,
    last: bool,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
}

/// `frame` as an Entries frame.
fn entries_of(frame: &[u8]) -> TestResult<Pushed> {
    let view = viewed(frame)?;
    if view.command() != Command::Entries {
        return Err(TestError::new(format!(
            "an Entries frame, not command {}",
            view.command().number()
        )));
    }
    let entries = EntriesView::from_frame(&view)?;
    Ok(Pushed {
        range: entries.range(),
        through: entries.through(),
        end: entries.end().number(),
        epoch: entries.epoch(),
        last: view.header().flags() & FLAG_LAST != 0,
        entries: entries
            .entries()
            .iter()
            .map(|entry| {
                (
                    entry.key().to_vec(),
                    entry.value().unwrap_or_default().to_vec(),
                )
            })
            .collect(),
    })
}

/// `frame` as an Error frame: its code, detail and message.
fn error_of(frame: &[u8]) -> TestResult<(ErrorCode, u64, String)> {
    let view = viewed(frame)?;
    let error = ErrorReplyView::from_frame(&view)?;
    Ok((error.code(), error.detail(), error.message().to_string()))
}

/// One dictionary run names each term's ID in one frame, and a forward list read at once names its object by that ID.
#[test]
fn test_a_scan_reads_at_once_what_was_committed() -> TestResult {
    with_core("reads-latest", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &edges(TENANT, &[("a", "p", "b")])).await?;
        let terms = ["a", "p", "b"].map(|name| alone(term_key(TENANT, &ex(name))));
        let (run, refused) = scanned(&mut client, &plain(ScanAt::Latest, 0, 0, &terms)).await?;
        assert!(refused.is_none(), "{refused:?}");
        let [batch] = run.as_slice() else {
            return Err(TestError::new(format!("{} frames of a run", run.len())));
        };
        assert_eq!((batch.range, batch.through), (0, 2));
        assert_eq!(batch.end, EntriesEnd::Ended.number());
        let ids = batch
            .entries
            .iter()
            .map(|(_, value)| SystemId::decode(value))
            .collect::<Result<Vec<_>, _>>()?;
        let [a, p, b] = ids.as_slice() else {
            return Err(TestError::new(format!("{} IDs", ids.len())));
        };
        let (lists, lists_refused) =
            scanned(&mut client, &plain(ScanAt::Latest, 0, 0, &[forward()])).await?;
        assert!(lists_refused.is_none(), "{lists_refused:?}");
        let list = lists.last().context("a frame of the list")?;
        let [(key, value)] = list.entries.as_slice() else {
            return Err(TestError::new(format!("{} lists", list.entries.len())));
        };
        let expected = QuadKey::Forward {
            tenant: TENANT,
            graph: Graph::Default,
            predicate: *p,
            subject: *a,
        };
        assert_eq!(key, &expected.to_bytes());
        let records = ObjectRecords::read(value)?;
        let objects: Vec<&[u8]> = records.iter().map(|record| record.encoding()).collect();
        assert_eq!(objects, vec![EncodedTerm::id(*b).as_bytes()]);
        Ok(())
    })
}

/// A term no commit wrote has no entry in the run, which still covers its range.
#[test]
fn test_a_dictionary_run_covers_a_term_never_written() -> TestResult {
    with_core("reads-absent", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &edges(TENANT, &[("a", "p", "b")])).await?;
        let terms = ["a", "never"].map(|name| alone(term_key(TENANT, &ex(name))));
        let (run, refused) = scanned(&mut client, &plain(ScanAt::Latest, 0, 0, &terms)).await?;
        assert!(refused.is_none(), "{refused:?}");
        let batch = run.first().context("a frame")?;
        assert_eq!((batch.range, batch.through), (0, 1));
        assert_eq!(batch.entries.len(), 1);
        Ok(())
    })
}

/// A Scan naming an epoch its partition has not closed, with no time to wait, is refused at once: code 13, its detail the
/// partition (`PROTOCOL.md` §13.13).
#[test]
fn test_a_scan_of_an_epoch_not_closed_with_no_time_to_wait_is_refused() -> TestResult {
    with_core("reads-no-wait", RigSetup::default(), async |core| {
        let mut direct = Direct::open(&core.inbox, DIRECT, LARGE)?;
        let answer = direct
            .request(1, &plain(ScanAt::Latest, 5, 0, &[forward()]))
            .await?;
        let (code, detail, message) = refusal(&answer)?;
        assert_eq!(code, ErrorCode::TimedOut);
        assert_eq!(detail, u64::from(PARTITION));
        assert!(message.contains("epoch 5"), "{message}");
        Ok(())
    })
}

/// A Scan naming an epoch its partition has not closed waits for it until its deadline, and then is refused.
#[test]
fn test_a_scan_waits_for_its_epoch_until_its_deadline() -> TestResult {
    with_core("reads-deadline", RigSetup::default(), async |core| {
        let mut direct = Direct::open(&core.inbox, DIRECT, LARGE)?;
        let answer = direct
            .request(1, &plain(ScanAt::Latest, 5, 1_500, &[forward()]))
            .await?;
        assert!(matches!(answer, Answer::Open), "{answer:?}");
        direct.nothing_for(ms(750)).await?;
        let (stream, frame) = direct.pushed().await?;
        assert_eq!(stream, 1);
        let (code, detail, message) = error_of(&frame)?;
        assert_eq!((code, detail), (ErrorCode::TimedOut, u64::from(PARTITION)));
        assert!(message.contains("epoch 5"), "{message}");
        Ok(())
    })
}

/// A Scan waiting for its epoch is answered once that epoch closes, its frames naming it.
#[test]
fn test_a_scan_waiting_for_its_epoch_starts_once_it_closes() -> TestResult {
    with_core("reads-epoch", RigSetup::default(), async |core| {
        let mut direct = Direct::open(&core.inbox, DIRECT, LARGE)?;
        let answer = direct
            .request(1, &plain(ScanAt::LatestClosed, 1, 9_000, &[forward()]))
            .await?;
        assert!(matches!(answer, Answer::Open), "{answer:?}");
        direct.nothing_for(ms(200)).await?;
        let close = Close::new(Epoch::new(1)?, 0, Vec::new(), Vec::new())?;
        core.closes.send(PartitionMessage::Close(close))?;
        let (_, frame) = direct.pushed().await?;
        let pushed = entries_of(&frame)?;
        assert_eq!(pushed.epoch, 1);
        assert!(pushed.last);
        Ok(())
    })
}

/// Closed names the partition's latest closed epoch, and refuses a partition this address does not host.
#[test]
fn test_closed_names_the_latest_closed_epoch_of_its_own_partition_alone() -> TestResult {
    with_core("reads-closed", RigSetup::default(), async |core| {
        let mut direct = Direct::open(&core.inbox, DIRECT, LARGE)?;
        for epoch in 1..=2 {
            let close = Close::new(Epoch::new(epoch)?, 0, Vec::new(), Vec::new())?;
            core.closes.send(PartitionMessage::Close(close))?;
        }
        let mut closed = 0;
        for _ in 0..200 {
            let answer = direct
                .request(
                    1,
                    &ultragraphdb_protocol_wire::Closed::request(&[PARTITION]),
                )
                .await?;
            let Answer::Reply(frame) = answer else {
                return Err(TestError::new(format!("a reply, not {answer:?}")));
            };
            let view = viewed(&frame)?;
            closed = ClosedView::from_frame(&view)?
                .partition(0)
                .context("the partition")?
                .epoch();
            if closed == 2 {
                break;
            }
            compio::time::sleep(ms(5)).await;
        }
        assert_eq!(closed, 2);
        let answer = direct
            .request(
                2,
                &ultragraphdb_protocol_wire::Closed::request(&[PARTITION + 1]),
            )
            .await?;
        let (code, _, message) = refusal(&answer)?;
        assert_eq!(code, ErrorCode::Protocol);
        assert!(
            message.contains(&format!("not {}", PARTITION + 1)),
            "{message}"
        );
        Ok(())
    })
}

/// A range larger than a turn's frame takes frames, the next built only once the one before left with credit for it.
#[test]
fn test_a_large_range_takes_frames_each_once_credit_allows() -> TestResult {
    with_core("reads-frames", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &large()).await?;
        let mut direct = Direct::open(&core.inbox, DIRECT, SMALL)?;
        let answer = direct
            .request(1, &plain(ScanAt::Latest, 0, 0, &[forward()]))
            .await?;
        assert!(matches!(answer, Answer::Open), "{answer:?}");
        let mut lists = 0;
        for frames in 1..=SUBJECTS {
            let (_, frame) = direct.pushed().await?;
            let pushed = entries_of(&frame)?;
            lists += pushed.entries.len();
            if pushed.last {
                assert!(frames > 1, "the range fit one frame");
                assert_eq!(pushed.end, EntriesEnd::Ended.number());
                assert_eq!(lists, SUBJECTS);
                return Ok(());
            }
            assert_eq!(pushed.end, EntriesEnd::More.number());
            // No frame is built while the one before holds no credit to leave on.
            direct.nothing_for(ms(150)).await?;
            direct.ready(1)?;
        }
        Err(TestError::new("the range never ended"))
    })
}

/// A Scan's limit counts the entries of every frame its range took: the range stops at it.
#[test]
fn test_a_limit_counts_the_entries_of_every_frame_of_its_range() -> TestResult {
    with_core("reads-limit", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &large()).await?;
        let limit = SUBJECTS - SUBJECTS / 4;
        let scan = limited(ScanAt::Latest, 0, 0, u32::try_from(limit)?, &[forward()]);
        let mut direct = Direct::open(&core.inbox, DIRECT, SMALL)?;
        let _open = direct.request(1, &scan).await?;
        let (mut lists, mut frames) = (0, 0);
        for _ in 0..SUBJECTS {
            let (_, frame) = direct.pushed().await?;
            let pushed = entries_of(&frame)?;
            lists += pushed.entries.len();
            frames += 1;
            if pushed.last {
                assert!(frames > 1, "the limit fell within the first frame");
                assert_eq!(lists, limit);
                assert_eq!(pushed.end, EntriesEnd::Limit.number());
                return Ok(());
            }
            direct.ready(1)?;
        }
        Err(TestError::new("the range never ended"))
    })
}

/// A Scan whose frame left with no credit for the next is refused as stalled once no frame could leave for its stall time, counted
/// from when its last frame left (`PROTOCOL.md` §11.9).
#[test]
fn test_a_stall_counts_from_when_the_last_frame_left() -> TestResult {
    let setup = RigSetup {
        stall_ms: 1_000,
        ..RigSetup::default()
    };
    with_core("reads-stall", setup, async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &large()).await?;
        let mut direct = Direct::open(&core.inbox, DIRECT, SMALL)?;
        let _open = direct
            .request(1, &plain(ScanAt::Latest, 0, 0, &[forward()]))
            .await?;
        let (_, frame) = direct.pushed().await?;
        assert!(!entries_of(&frame)?.last);
        direct.left(1)?;
        direct.nothing_for(ms(600)).await?;
        // The frame left only now: the stall counts from here.
        direct.left(1)?;
        direct.nothing_for(ms(700)).await?;
        let (stream, stalled) = direct.pushed().await?;
        assert_eq!(stream, 1);
        let (code, detail, message) = error_of(&stalled)?;
        assert_eq!((code, detail), (ErrorCode::TimedOut, u64::from(PARTITION)));
        assert!(message.contains("stalled"), "{message}");
        Ok(())
    })
}

/// A Scan its reader cancelled builds no more frames, though credit arrives for them.
#[test]
fn test_a_cancelled_scan_builds_no_more_frames() -> TestResult {
    with_core("reads-cancel", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &large()).await?;
        let mut direct = Direct::open(&core.inbox, DIRECT, SMALL)?;
        let _open = direct
            .request(1, &plain(ScanAt::Latest, 0, 0, &[forward()]))
            .await?;
        let (_, frame) = direct.pushed().await?;
        assert!(!entries_of(&frame)?.last);
        direct.cancel(1)?;
        direct.ready(1)?;
        direct.nothing_for(ms(500)).await
    })
}

/// The Scans of a connection that closed build no more frames.
#[test]
fn test_a_closed_connections_scans_build_no_more_frames() -> TestResult {
    with_core(
        "reads-closed-connection",
        RigSetup::default(),
        async |core| {
            let mut client = core.dial().await?;
            let _version = commit(&mut client, &large()).await?;
            let mut direct = Direct::open(&core.inbox, DIRECT, SMALL)?;
            let _open = direct
                .request(1, &plain(ScanAt::Latest, 0, 0, &[forward()]))
                .await?;
            let (_, frame) = direct.pushed().await?;
            assert!(!entries_of(&frame)?.last);
            direct.close()?;
            direct.ready(1)?;
            direct.nothing_for(ms(500)).await
        },
    )
}

/// The tenant's whole statements keyspace, as one range.
fn statements() -> (Vec<u8>, Vec<u8>) {
    (
        key_prefix(Keyspace::Statements, false, TENANT, None, None),
        Vec::new(),
    )
}

/// A DirectWrite of `<a> <p> <b>` with the edge's property `<since>` "2019", which gives the edge a statement's record.
fn edge_with_a_property() -> TestResult<DirectWrite> {
    let mut terms = TermTable::new();
    let change = WriteChange::InsertEdge {
        graph: None,
        subject: terms.iri(&ex("a")),
        predicate: terms.iri(&ex("p")),
        object: terms.iri(&ex("b")),
        properties: vec![(
            terms.iri(&ex("since")),
            terms.intern(RequestTerm::Literal {
                lexical: "2019".to_string(),
                datatype: None,
            })?,
        )],
    };
    Ok(DirectWrite::new(TENANT, terms, vec![change]).with_principal(PRINCIPAL))
}

/// Every entry of the Scan answering on `stream` of `direct`, its frames taken as each leaves with credit for the next.
async fn every_entry(direct: &mut Direct, stream: u64) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut entries = Vec::new();
    for _ in 0..SUBJECTS {
        let (_, frame) = direct.pushed().await?;
        let pushed = entries_of(&frame)?;
        entries.extend(pushed.entries);
        if pushed.last {
            return Ok(entries);
        }
        direct.ready(stream)?;
    }
    Err(TestError::new("the Scan never ended"))
}

/// A Scan of a transaction reads the IDs of the terms it names, a forward list and a statement's record, each as its
/// transaction sees them (`PROTOCOL.md` §13.13).
#[test]
fn test_a_transactions_scan_reads_terms_lists_and_records() -> TestResult {
    with_core("reads-bound", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &edge_with_a_property()?).await?;
        let mut direct = Direct::open(&core.inbox, DIRECT, LARGE)?;
        let identity = [4; 16];
        let bind = Bind::new(
            PARTITION,
            BindKind::Transaction,
            TENANT,
            0,
            PRINCIPAL,
            identity,
        );
        let Answer::Reply(bound) = direct.request(1, &bind).await? else {
            return Err(TestError::new("the Bind's reply"));
        };
        let read = BindView::from_frame(&viewed(&bound)?)?.read_version();
        let mut terms = TermTable::new();
        let mut scan = Scan::new(ScanAt::ReadVersion, read, 0, PRINCIPAL, identity, 0, 0);
        for name in ["a", "p", "b"] {
            let _at = terms.iri(&ex(name));
            let (first, past) = alone(term_key(TENANT, &ex(name)));
            scan.push_range(ScanRange::new(PARTITION, &first, &past));
        }
        for (first, past) in [forward(), statements()] {
            scan.push_range(ScanRange::new(PARTITION, &first, &past));
        }
        let answer = direct.request(2, &scan.with_terms(terms)).await?;
        assert!(matches!(answer, Answer::Open), "{answer:?}");
        let entries = every_entry(&mut direct, 2).await?;
        let [
            (_, a),
            (_, p),
            (_, b),
            (list_key, list),
            (record_key, record),
        ] = entries.as_slice()
        else {
            return Err(TestError::new(format!("{} entries", entries.len())));
        };
        let (a, p, b) = (
            SystemId::decode(a)?,
            SystemId::decode(p)?,
            SystemId::decode(b)?,
        );
        let expected = QuadKey::Forward {
            tenant: TENANT,
            graph: Graph::Default,
            predicate: p,
            subject: a,
        };
        assert_eq!(list_key, &expected.to_bytes());
        let objects: Vec<Vec<u8>> = ObjectRecords::read(list)?
            .iter()
            .map(|object| object.encoding().to_vec())
            .collect();
        assert_eq!(objects, vec![EncodedTerm::id(b).as_bytes().to_vec()]);
        let held = StatementRecord::decode(record_key, record)?;
        assert_eq!((held.subject(), held.predicate()), (a, p));
        assert_eq!(held.object(), &EncodedTerm::id(b));
        Ok(())
    })
}

/// A Scan of no transaction reads a statement's record by its value.
#[test]
fn test_a_scan_reads_a_statements_record() -> TestResult {
    with_core("reads-record", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &edge_with_a_property()?).await?;
        let (batches, refused) =
            scanned(&mut client, &plain(ScanAt::Latest, 0, 0, &[statements()])).await?;
        assert!(refused.is_none(), "{refused:?}");
        let entries: Vec<&(Vec<u8>, Vec<u8>)> = batches
            .iter()
            .flat_map(|batch| batch.entries.iter())
            .collect();
        let [(key, value)] = entries.as_slice() else {
            return Err(TestError::new(format!("{} records", entries.len())));
        };
        let held = StatementRecord::decode(key, value)?;
        assert_eq!(held.properties().len(), 1);
        Ok(())
    })
}

/// A page finding only keys holding nothing sends no frame: the Scan goes on at its next turn, and its first frame holds what
/// the range holds after them (`PROTOCOL.md` §13.14).
#[test]
fn test_a_page_of_keys_holding_nothing_sends_no_frame() -> TestResult {
    with_core("reads-idle", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &large()).await?;
        let mut terms = TermTable::new();
        let (predicate, object) = (terms.iri(&ex("p")), terms.iri(&ex("o")));
        let deletes = (0..SUBJECTS)
            .map(|at| WriteChange::Delete {
                graph: None,
                subject: terms.iri(&ex(&format!("s{at}"))),
                predicate,
                object,
            })
            .collect();
        let emptied = DirectWrite::new(TENANT, terms, deletes).with_principal(PRINCIPAL);
        let _emptied_at = commit(&mut client, &emptied).await?;
        let mut direct = Direct::open(&core.inbox, DIRECT, LARGE)?;
        let _open = direct
            .request(1, &plain(ScanAt::Latest, 0, 0, &[forward()]))
            .await?;
        let (_, frame) = direct.pushed().await?;
        let pushed = entries_of(&frame)?;
        assert!(pushed.last, "a frame was sent before the range ended");
        assert_eq!(pushed.end, EntriesEnd::Ended.number());
        assert!(pushed.entries.is_empty());
        Ok(())
    })
}

/// Two Scans of a transaction, each naming an IRI whose home has not answered, both wait on it, and both start once it answers.
#[test]
fn test_scans_of_a_transaction_wait_on_its_home_together() -> TestResult {
    use crate::types::partition_core::played_cores;
    played_cores::served(2, async |mut driver| {
        let connection = driver.connect(DIRECT, 1 << 20)?;
        let identity = 6;
        let bound = played_cores::reply_of(
            driver
                .ask(
                    &connection,
                    1,
                    &played_cores::bind(BindKind::Transaction, identity),
                )
                .await?,
        )?;
        let read = BindView::from_frame(&viewed_as_played(&bound)?)?.read_version();
        let foreign = driver.owned_by(2, "x")?;
        let mut answers = Vec::new();
        for stream in [2, 3] {
            let mut terms = TermTable::new();
            let _at = terms.iri(&foreign);
            let (first, past) = alone(term_key(played_cores::TENANT, &foreign));
            let mut scan = Scan::new(
                ScanAt::ReadVersion,
                read,
                0,
                played_cores::PRINCIPAL,
                [identity; 16],
                0,
                0,
            );
            scan.push_range(ScanRange::new(PARTITION, &first, &past));
            answers.push(driver.send(&connection, stream, &scan.with_terms(terms))?);
        }
        for answer in answers {
            let answer = played_cores::answered(answer).await?;
            assert!(matches!(answer, Answer::Open), "{answer:?}");
        }
        let mut connection = connection;
        assert!(!connection.holds_push(), "a frame before the home answered");
        driver.release_homes();
        let mut ended = Vec::new();
        for _ in 0..2 {
            let (stream, frame) = connection
                .pushed("a Scan's frame once its home answered")
                .await?;
            let view = played_cores_view(&frame)?;
            assert_eq!(view.command(), Command::Entries);
            assert!(view.header().flags() & FLAG_LAST != 0);
            ended.push(stream);
        }
        ended.sort_unstable();
        assert_eq!(ended, vec![2, 3]);
        Ok(())
    })
}

/// `frame`, a frame of the played cores' cluster, viewed.
fn played_cores_view(frame: &[u8]) -> TestResult<ultragraphdb_protocol_wire::FrameView<'_>> {
    Ok(ultragraphdb_protocol_wire::FrameView::validate(
        frame,
        crate::types::partition_core::played_cores::CLUSTER,
        MAX_FRAME_SIZE,
    )?)
}

/// `frame`, a reply of the played cores, viewed.
fn viewed_as_played(frame: &[u8]) -> TestResult<ultragraphdb_protocol_wire::FrameView<'_>> {
    played_cores_view(frame)
}

/// A home in another process refusing the registration a transaction's Scan waits on ends the transaction in either case: the
/// Scan's stream and the Bind's each carry outcome 26 as code 26, `detail` 0 (`PROTOCOL.md` §13.15).
#[test]
fn test_a_homes_refusal_ends_the_waiting_scan_and_its_transaction() -> TestResult {
    use crate::types::partition_core::played_cores;
    played_cores::served_beside_remote(async |driver, home| {
        let mut connection = driver.connect(DIRECT, 1 << 20)?;
        let identity = 8;
        let bind = played_cores::bind(BindKind::Transaction, identity);
        let bound = played_cores::reply_of(driver.ask(&connection, 1, &bind).await?)?;
        let read = BindView::from_frame(&viewed_as_played(&bound)?)?.read_version();
        let foreign = driver.owned_by(2, "x")?;
        let mut terms = TermTable::new();
        let _at = terms.iri(&foreign);
        let (first, past) = alone(term_key(played_cores::TENANT, &foreign));
        let principal = played_cores::PRINCIPAL;
        let mut scan = Scan::new(
            ScanAt::ReadVersion,
            read,
            0,
            principal,
            [identity; 16],
            0,
            0,
        );
        scan.push_range(ScanRange::new(PARTITION, &first, &past));
        let answer = driver.ask(&connection, 2, &scan.with_terms(terms)).await?;
        assert!(matches!(answer, Answer::Open), "{answer:?}");
        let (write, indexes) = home.registered().await?;
        assert!(!indexes.is_empty(), "a registration of the Scan's IRI");
        for index in indexes {
            home.refuse(write, index, ErrorCode::WrongPartition.number())?;
        }
        let mut ended = Vec::new();
        for _ in 0..2 {
            let (stream, frame) = connection
                .pushed("each stream's end once the home refused")
                .await?;
            let error = played_cores::error_in(&frame)?.context("an Error frame")?;
            ended.push((stream, error.code(), error.detail()));
        }
        ended.sort_unstable_by_key(|(stream, _, _)| *stream);
        let refused = |stream| (stream, ErrorCode::WrongPartition, 0);
        assert_eq!(ended, [refused(1), refused(2)]);
        Ok(())
    })
}

/// A Recall answers a committed one-frame write's reply from the session table, as the Commit answered it, and refuses a
/// partition this address does not host (`PROTOCOL.md` §6.6).
#[test]
fn test_a_recall_answers_the_reply_its_session_holds() -> TestResult {
    with_core("reads-recall", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let session = [0x3c; 16];
        let write = edges(TENANT, &[("a", "p", "b")]).with_session(session, 4);
        let version = commit(&mut client, &write).await?;
        let mut direct = Direct::open(&core.inbox, DIRECT, LARGE)?;
        let answer = direct
            .request(
                1,
                &ultragraphdb_protocol_wire::Recall::new(PARTITION, session, 4),
            )
            .await?;
        let Answer::Reply(frame) = answer else {
            return Err(TestError::new(format!("a reply, not {answer:?}")));
        };
        let view = viewed(&frame)?;
        let recalled = ultragraphdb_protocol_wire::CommitView::from_frame(&view)?;
        assert_eq!((recalled.session(), recalled.request()), (session, 4));
        assert_eq!(recalled.commit_version(), version);
        let other = ultragraphdb_protocol_wire::Recall::new(PARTITION + 1, session, 4);
        let (code, _, message) = refusal(&direct.request(2, &other).await?)?;
        assert_eq!(code, ErrorCode::Protocol);
        assert!(
            message.contains(&format!("not {}", PARTITION + 1)),
            "{message}"
        );
        Ok(())
    })
}

/// A run of dictionary ranges larger than a frame takes frames, each holding every range that fits and the next beginning
/// where the one before stopped (`PROTOCOL.md` §13.14).
#[test]
fn test_a_dictionary_run_larger_than_a_frame_takes_frames() -> TestResult {
    with_core("reads-long-run", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let _version = commit(&mut client, &large()).await?;
        let terms: Vec<(Vec<u8>, Vec<u8>)> = (0..SUBJECTS)
            .map(|at| alone(term_key(TENANT, &ex(&format!("s{at}")))))
            .collect();
        let mut direct = Direct::open(&core.inbox, DIRECT, SMALL)?;
        let answer = direct
            .request(1, &plain(ScanAt::Latest, 0, 0, &terms))
            .await?;
        assert!(matches!(answer, Answer::Open), "{answer:?}");
        let (mut next, mut ids, mut frames) = (0, 0, 0);
        for _ in 0..SUBJECTS {
            let (_, frame) = direct.pushed().await?;
            let pushed = entries_of(&frame)?;
            assert_eq!(
                pushed.range, next,
                "a frame begins where the one before stopped"
            );
            next = pushed.through + 1;
            ids += pushed.entries.len();
            frames += 1;
            if pushed.last {
                assert!(frames > 1, "the run fit one frame");
                assert_eq!((ids, usize::try_from(next)?), (SUBJECTS, SUBJECTS));
                return Ok(());
            }
            direct.ready(1)?;
        }
        Err(TestError::new("the run never ended"))
    })
}
