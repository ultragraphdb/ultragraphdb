/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! How a partition's core takes the requests its connections hand it (`PROTOCOL.md` §11.2, §11.12), played through its inbox.

use ultragraphdb_common_role::Answer;
use ultragraphdb_common_storage::CoreMessage;
use ultragraphdb_protocol_wire::{
    Abort, Bind, BindKind, BindView, BodyView, Command, Commit, CommitOperation, CommitView,
    DropCommand, ErrorCode, FrameView, Lease, MAX_FRAME_SIZE, Operation, Scan, ScanAt, ScanRange,
    TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::partition_core::played_cores::{
    CLUSTER, Driver, PRINCIPAL, TENANT, answered, bind, command_of, error_in, refusal_of, reply_of,
    served, served_at,
};

/// The IRI the tenant's first write names, from its home (`PROTOCOL.md` §11.12).
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// How long a test watches for a frame that must not come.
const SILENCE: std::time::Duration = std::time::Duration::from_millis(200);

/// A Write inserting one edge from an IRI partition 1 owns to one `partition` owns.
pub(crate) fn edge(driver: &Driver, partition: u32, at: &str) -> TestResult<Write> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.iri(&driver.owned_by(1, &format!("{at}-s"))?),
        predicate: terms.iri(&driver.owned_by(1, "knows")?),
        object: terms.iri(&driver.owned_by(partition, &format!("{at}-o"))?),
    };
    Ok(Write::new(terms, vec![change]))
}

/// A transaction bound, written and committed on one stream: Bind's reply naming its read version, Held, and the Commit pushed
/// once durable, ending the stream.
#[test]
fn test_a_transaction_binds_writes_and_commits_on_its_stream() -> TestResult {
    served(1, async |driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let view = FrameView::validate(&bound, CLUSTER, MAX_FRAME_SIZE)?;
        assert_eq!(view.command(), Command::Bind);
        assert_eq!(BindView::from_frame(&view)?.tenant(), TENANT);
        let held = reply_of(driver.ask(&connection, 3, &edge(&driver, 1, "a")?).await?)?;
        assert_eq!(command_of(&held)?, Command::Held);
        let commit = Commit::new([9; 16], 1, 0);
        assert_eq!(driver.ask(&connection, 3, &commit).await?, Answer::Open);
        let (stream, frame) = connection.pushed("the Commit's reply").await?;
        assert_eq!(stream, 3);
        let reply = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
        let committed = CommitView::from_frame(&reply)?;
        assert_eq!((committed.session(), committed.request()), ([9; 16], 1));
        assert!(committed.commit_version() > 0);
        Ok(())
    })
}

/// A command the partition does not host is refused as unavailable, whatever handles the others.
#[test]
fn test_a_command_not_hosted_here_is_refused_as_unavailable() -> TestResult {
    served(1, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let lease = Lease::request(1, 1, 16);
        let refused = refusal_of(driver.ask(&connection, 3, &lease).await?)?;
        assert_eq!(
            refused.code(),
            ErrorCode::Unavailable,
            "{}",
            refused.message()
        );
        let number = Command::Lease.number();
        assert_eq!(
            refused.message(),
            format!("command {number} is not hosted here yet")
        );
        Ok(())
    })
}

/// An Abort, a Write or a Commit on a stream no Bind of kind 1 opened is refused naming the kind (`PROTOCOL.md` §11.2); an Abort
/// of a bound transaction ends it with no frame.
#[test]
fn test_a_transactions_requests_need_its_bind() -> TestResult {
    served(1, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let unbound = |command: &str| format!("{command} on a stream no Bind of kind 1 opened");
        let refused = refusal_of(driver.ask(&connection, 3, &Abort).await?)?;
        assert_eq!(
            (refused.code(), refused.message()),
            (ErrorCode::Protocol, unbound("Abort").as_str())
        );
        let write = refusal_of(driver.ask(&connection, 5, &edge(&driver, 1, "w")?).await?)?;
        assert_eq!(write.message(), unbound("Write"));
        let commit = refusal_of(
            driver
                .ask(&connection, 7, &Commit::new([0; 16], 0, 0))
                .await?,
        )?;
        assert_eq!(commit.message(), unbound("Commit"));
        let _bound = reply_of(
            driver
                .ask(&connection, 9, &bind(BindKind::Transaction, 2))
                .await?,
        )?;
        assert_eq!(driver.ask(&connection, 9, &Abort).await?, Answer::Done);
        let ended = refusal_of(driver.ask(&connection, 9, &Abort).await?)?;
        assert_eq!(ended.message(), unbound("Abort"), "the Abort ended it");
        Ok(())
    })
}

/// A Bind naming another partition, or a tenant the placement map lists no run for, is refused saying which.
#[test]
fn test_a_bind_names_this_partition_and_a_listed_tenant() -> TestResult {
    served(1, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let elsewhere = Bind::new(2, BindKind::Transaction, TENANT, 0, 3, [4; 16]);
        let refused = refusal_of(driver.ask(&connection, 3, &elsewhere).await?)?;
        assert_eq!(
            (refused.code(), refused.message()),
            (ErrorCode::Protocol, "this address hosts partition 1, not 2")
        );
        let unlisted = TENANT + 1;
        let other = Bind::new(1, BindKind::Transaction, unlisted, 0, 3, [4; 16]);
        let unlisted_refused = refusal_of(driver.ask(&connection, 5, &other).await?)?;
        assert_eq!(
            unlisted_refused.message(),
            format!("this partition's placement map lists no run for tenant {unlisted}")
        );
        Ok(())
    })
}

/// A Drop stream of one connection outlives another connection's close: its Drop runs and is answered once committed.
#[test]
fn test_a_connections_close_leaves_anothers_drop_stream() -> TestResult {
    served(1, async |driver| {
        let mut kept = driver.connect(1, 1 << 20)?;
        let closing = driver.connect(2, 1 << 20)?;
        let _bound = reply_of(driver.ask(&kept, 3, &bind(BindKind::Drop, 1)).await?)?;
        let _other = reply_of(driver.ask(&closing, 3, &bind(BindKind::Drop, 2)).await?)?;
        driver.tell(CoreMessage::Closed {
            connection: closing.id(),
        })?;
        let drop = DropCommand::tenant_data(TENANT).with_session([5; 16], 1);
        assert_eq!(driver.ask(&kept, 3, &drop).await?, Answer::Open);
        let (stream, frame) = kept.pushed("the Drop's reply").await?;
        assert_eq!(stream, 3);
        assert_eq!(
            command_of(&frame)?,
            Command::Drop,
            "{:?}",
            error_in(&frame)?
        );
        Ok(())
    })
}

/// What follows a parked Write on its stream waits behind it, copied and counted against its connection, and goes on in its
/// order once the home answers: Held, then the Commit. Another connection's close leaves it parked, and nothing stays counted.
#[test]
fn test_what_follows_a_parked_write_keeps_its_order() -> TestResult {
    served(2, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let closing = driver.connect(2, 1 << 20)?;
        let _bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let write = driver.send(&connection, 3, &edge(&driver, 2, "parked")?)?;
        let commit = driver.send(&connection, 3, &Commit::new([6; 16], 1, 0))?;
        assert_eq!(answered(write).await?, Answer::Open, "the Write parks");
        assert_eq!(
            answered(commit).await?,
            Answer::Open,
            "the Commit waits behind it"
        );
        assert!(connection.parked() > 0, "both frames are counted");
        assert!(!connection.holds_push(), "nothing answers before the home");
        driver.tell(CoreMessage::Closed {
            connection: closing.id(),
        })?;
        driver.release_homes();
        let (held_on, held) = connection.pushed("the Write's Held").await?;
        assert_eq!(
            (held_on, command_of(&held)?),
            (3, Command::Held),
            "{:?}",
            error_in(&held)?
        );
        let (committed_on, committed) = connection.pushed("the Commit's reply").await?;
        assert_eq!(committed_on, 3);
        let view = FrameView::validate(&committed, CLUSTER, MAX_FRAME_SIZE)?;
        let reply = CommitView::from_frame(&view).context("a Commit")?;
        assert_eq!((reply.session(), reply.request()), ([6; 16], 1));
        assert_eq!(connection.parked(), 0, "nothing stays counted");
        Ok(())
    })
}

/// A Scan on partition 2 of the transaction `identity` read at `version`, looking up `iri`'s dictionary entry.
fn term_scan(identity: u8, version: u64, iri: &str) -> Scan {
    let mut terms = TermTable::new();
    let _at = terms.iri(iri);
    let mut scan = Scan::new(
        ScanAt::ReadVersion,
        version,
        0,
        PRINCIPAL,
        [identity; 16],
        0,
        0,
    )
    .with_terms(terms);
    let first = term_key(TENANT, iri);
    let past = [first.as_slice(), &[0]].concat();
    scan.push_range(ScanRange::new(2, &first, &past));
    scan
}

/// A Write and a Commit of a transaction whose Scan waits on a home's answer wait their turn behind it (`PROTOCOL.md` §13.13),
/// though the Write names nothing of another partition: nothing answers before the home, and then Held and the Commit in their
/// order, beside the read's answer. Partition 2 is tested, which homes the IRI a tenant's first write names.
#[test]
fn test_a_transactions_writes_wait_behind_its_reads_home() -> TestResult {
    served_at(2, 2, async |mut driver| {
        assert_eq!(
            driver.home_of(RDF_TYPE)?,
            2,
            "the first write needs no home"
        );
        let mut connection = driver.connect(1, 1 << 20)?;
        let bind = Bind::new(2, BindKind::Transaction, TENANT, 0, PRINCIPAL, [7; 16]);
        let bound = reply_of(driver.ask(&connection, 3, &bind).await?)?;
        let read = BindView::from_frame(&FrameView::validate(&bound, CLUSTER, MAX_FRAME_SIZE)?)?
            .read_version();
        let scan = term_scan(7, read, &driver.owned_by(1, "read")?);
        assert_eq!(driver.ask(&connection, 5, &scan).await?, Answer::Open);
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(&driver.owned_by(2, "behind-s")?),
            predicate: terms.iri(&driver.owned_by(2, "knows")?),
            object: terms.iri(&driver.owned_by(2, "behind-o")?),
        };
        let write = Write::new(terms, vec![change]);
        assert_eq!(
            driver.ask(&connection, 3, &write).await?,
            Answer::Open,
            "the Write waits"
        );
        let commit = Commit::new([4; 16], 1, 0);
        assert_eq!(driver.ask(&connection, 3, &commit).await?, Answer::Open);
        compio::time::sleep(SILENCE).await;
        assert!(!connection.holds_push(), "nothing answers before the home");
        driver.release_homes();
        let mut on_three = Vec::new();
        for what in [
            "the read's Entries",
            "the Write's Held",
            "the Commit's reply",
        ] {
            let (stream, frame) = connection.pushed(what).await?;
            let command = command_of(&frame)?;
            if stream == 3 {
                on_three.push(command);
            } else {
                // The read's answer, or its end with the transaction the Commit decided, whichever came first.
                assert_eq!(stream, 5);
                assert!(
                    matches!(command, Command::Entries | Command::Error),
                    "{command:?}"
                );
            }
        }
        assert_eq!(on_three, [Command::Held, Command::Commit]);
        Ok(())
    })
}

/// A Write on partition 2 whose terms partition 2 owns: answered at once once the tenant's first write is.
fn local_write(driver: &Driver, at: &str) -> TestResult<Write> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.iri(&driver.owned_by(2, &format!("{at}-s"))?),
        predicate: terms.iri(&driver.owned_by(2, "knows")?),
        object: terms.iri(&driver.owned_by(2, &format!("{at}-o"))?),
    };
    Ok(Write::new(terms, vec![change]))
}

/// A Commit arriving while its transaction's Scan waits on a home waits its turn behind the read, with nothing parked ahead of it:
/// nothing answers before the home, and then the Commit.
#[test]
fn test_a_commit_waits_behind_its_reads_home() -> TestResult {
    served_at(2, 2, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let bind = Bind::new(2, BindKind::Transaction, TENANT, 0, PRINCIPAL, [8; 16]);
        let bound = reply_of(driver.ask(&connection, 3, &bind).await?)?;
        let read = BindView::from_frame(&FrameView::validate(&bound, CLUSTER, MAX_FRAME_SIZE)?)?
            .read_version();
        let held = reply_of(
            driver
                .ask(&connection, 3, &local_write(&driver, "first")?)
                .await?,
        )?;
        assert_eq!(command_of(&held)?, Command::Held, "{:?}", error_in(&held)?);
        let scan = term_scan(8, read, &driver.owned_by(1, "read")?);
        assert_eq!(driver.ask(&connection, 5, &scan).await?, Answer::Open);
        let commit = Commit::new([4; 16], 1, 0);
        assert_eq!(driver.ask(&connection, 3, &commit).await?, Answer::Open);
        compio::time::sleep(SILENCE).await;
        assert!(!connection.holds_push(), "nothing answers before the home");
        driver.release_homes();
        let mut on_three = Vec::new();
        for what in ["the read's answer", "the Commit's reply"] {
            let (stream, frame) = connection.pushed(what).await?;
            if stream == 3 {
                on_three.push(command_of(&frame)?);
            }
        }
        assert_eq!(on_three, [Command::Commit]);
        Ok(())
    })
}

/// An Operation in a bound transaction is held as a Write is, and its result comes back with the Commit, pushed once
/// durable.
#[test]
fn test_a_transactions_operation_is_held_and_its_result_committed() -> TestResult {
    served(1, async |driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let _bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::CounterAdd {
            graph: None,
            subject: terms.iri(&driver.owned_by(1, "counter")?),
            predicate: terms.iri(&driver.owned_by(1, "count")?),
            delta: 2,
        };
        let held = reply_of(
            driver
                .ask(&connection, 3, &Operation::new(terms, operation))
                .await?,
        )?;
        assert_eq!(command_of(&held)?, Command::Held);
        let commit = Commit::new([9; 16], 1, 0);
        assert_eq!(driver.ask(&connection, 3, &commit).await?, Answer::Open);
        let (stream, frame) = connection.pushed("the Commit's reply").await?;
        assert_eq!(stream, 3);
        let reply = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
        assert_eq!(reply.command(), Command::Commit, "{:?}", error_in(&frame));
        let committed = CommitView::from_frame(&reply)?;
        assert_eq!(committed.result_count(), 1);
        Ok(())
    })
}

/// `frame` with its last byte changed: its header holds, its body's checksum fails.
fn corrupted(mut frame: Vec<u8>) -> TestResult<Vec<u8>> {
    let last = frame.last_mut().context("a frame's bytes")?;
    *last ^= 0xFF;
    Ok(frame)
}

/// A request whose body fails its checksum is refused as a protocol error, and one waiting behind a parked Write is refused
/// in its turn, once the Write is held, on its stream.
#[test]
fn test_a_request_failing_its_checksum_is_refused_now_or_in_its_turn() -> TestResult {
    use crate::types::partition_core::played_cores::framed;
    served(2, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let commit = corrupted(framed(&Commit::new([1; 16], 1, 0), 3)?)?;
        let refused =
            refusal_of(answered(driver.send_frame(&connection, 3, commit.clone())?).await?)?;
        assert_eq!(refused.code(), ErrorCode::Protocol, "{}", refused.message());
        let _bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let write = driver.send(&connection, 3, &edge(&driver, 2, "parked")?)?;
        assert_eq!(answered(write).await?, Answer::Open, "the Write parks");
        let behind = driver.send_frame(&connection, 3, commit)?;
        assert_eq!(answered(behind).await?, Answer::Open, "it waits behind");
        driver.release_homes();
        let (_, held) = connection.pushed("the Write's Held").await?;
        assert_eq!(command_of(&held)?, Command::Held, "{:?}", error_in(&held)?);
        let (stream, frame) = connection.pushed("the refusal in its turn").await?;
        let in_turn = error_in(&frame)?.context("an Error")?;
        assert_eq!((stream, in_turn.code()), (3, ErrorCode::Protocol));
        Ok(())
    })
}

/// A Write its transaction refuses, its first subject another partition's, ends the transaction: what follows on its stream
/// finds no Bind.
#[test]
fn test_a_refused_write_ends_its_transaction() -> TestResult {
    served(2, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let _bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(&driver.owned_by(2, "elsewhere-s")?),
            predicate: terms.iri(&driver.owned_by(1, "knows")?),
            object: terms.iri(&driver.owned_by(1, "elsewhere-o")?),
        };
        let refused = refusal_of(
            driver
                .ask(&connection, 3, &Write::new(terms, vec![change]))
                .await?,
        )?;
        assert_eq!(
            refused.code(),
            ErrorCode::WrongPartition,
            "{}",
            refused.message()
        );
        let ended = refusal_of(driver.ask(&connection, 3, &Abort).await?)?;
        assert_eq!(
            ended.message(),
            "Abort on a stream no Bind of kind 1 opened"
        );
        Ok(())
    })
}

/// A counter on `subject`'s `predicate`, its terms in `terms`.
fn counter(terms: &mut TermTable, subject: &str, predicate: &str) -> CommitOperation {
    CommitOperation::CounterAdd {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
        delta: 1,
    }
}

/// An Operation on a stream no Bind opened is refused naming the kind; one its transaction refuses, its predicate a
/// literal, ends the transaction.
#[test]
fn test_an_operation_needs_its_bind_and_a_refused_one_ends_it() -> TestResult {
    served(1, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let mut terms = TermTable::new();
        let operation = counter(
            &mut terms,
            &driver.owned_by(1, "c")?,
            &driver.owned_by(1, "n")?,
        );
        let unbound = refusal_of(
            driver
                .ask(&connection, 3, &Operation::new(terms, operation))
                .await?,
        )?;
        assert_eq!(
            (unbound.code(), unbound.message()),
            (
                ErrorCode::Protocol,
                "Operation on a stream no Bind of kind 1 opened"
            )
        );
        let _bound = reply_of(
            driver
                .ask(&connection, 5, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let mut literal_terms = TermTable::new();
        let literal = literal_terms.intern(ultragraphdb_protocol_wire::RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: None,
        })?;
        let literal_predicate = CommitOperation::CounterAdd {
            graph: None,
            subject: literal_terms.iri(&driver.owned_by(1, "c")?),
            predicate: literal,
            delta: 1,
        };
        let refused = refusal_of(
            driver
                .ask(
                    &connection,
                    5,
                    &Operation::new(literal_terms, literal_predicate),
                )
                .await?,
        )?;
        assert_eq!(refused.code(), ErrorCode::Protocol, "{}", refused.message());
        let ended = refusal_of(driver.ask(&connection, 5, &Abort).await?)?;
        assert_eq!(
            ended.message(),
            "Abort on a stream no Bind of kind 1 opened"
        );
        Ok(())
    })
}

/// An Operation naming a predicate homed on another partition parks until its home answers, and is then held.
#[test]
fn test_an_operation_waiting_on_its_home_is_held_once_answered() -> TestResult {
    served(2, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let _bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let mut terms = TermTable::new();
        let operation = counter(
            &mut terms,
            &driver.owned_by(1, "c")?,
            &driver.owned_by(2, "n")?,
        );
        assert_eq!(
            driver
                .ask(&connection, 3, &Operation::new(terms, operation))
                .await?,
            Answer::Open,
            "it parks"
        );
        driver.release_homes();
        let (stream, held) = connection.pushed("the Operation's Held").await?;
        assert_eq!(
            (stream, command_of(&held)?),
            (3, Command::Held),
            "{:?}",
            error_in(&held)?
        );
        Ok(())
    })
}

/// A Write waiting behind its transaction's read that names another partition's entity parks again in its turn, and is
/// held once that home answers too. Partition 2 is tested, which homes the IRI a tenant's first write names.
#[test]
fn test_a_write_behind_a_read_parks_again_on_its_own_home() -> TestResult {
    served_at(2, 2, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let bind = Bind::new(2, BindKind::Transaction, TENANT, 0, PRINCIPAL, [7; 16]);
        let bound = reply_of(driver.ask(&connection, 3, &bind).await?)?;
        let read = BindView::from_frame(&FrameView::validate(&bound, CLUSTER, MAX_FRAME_SIZE)?)?
            .read_version();
        let scan = term_scan(7, read, &driver.owned_by(1, "read")?);
        assert_eq!(driver.ask(&connection, 5, &scan).await?, Answer::Open);
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(&driver.owned_by(2, "again-s")?),
            predicate: terms.iri(&driver.owned_by(2, "knows")?),
            object: terms.iri(&driver.owned_by(1, "again-o")?),
        };
        let write = Write::new(terms, vec![change]);
        assert_eq!(driver.ask(&connection, 3, &write).await?, Answer::Open);
        driver.release_homes();
        let mut on_three = Vec::new();
        for what in ["the read's answer", "the Write's Held"] {
            let (stream, frame) = connection.pushed(what).await?;
            if stream == 3 {
                on_three.push(command_of(&frame)?);
            }
        }
        assert_eq!(on_three, [Command::Held]);
        Ok(())
    })
}

/// A parked Write its transaction refuses once its references are answered, a later subject another partition's, ends the
/// transaction on its stream.
#[test]
fn test_a_parked_write_refused_once_answered_ends_its_transaction() -> TestResult {
    served(2, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let _bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let mut terms = TermTable::new();
        let knows = terms.iri(&driver.owned_by(1, "knows")?);
        let changes = vec![
            WriteChange::Insert {
                graph: None,
                subject: terms.iri(&driver.owned_by(1, "first-s")?),
                predicate: knows,
                object: terms.iri(&driver.owned_by(2, "first-o")?),
            },
            WriteChange::Insert {
                graph: None,
                subject: terms.iri(&driver.owned_by(2, "later-s")?),
                predicate: knows,
                object: terms.iri(&driver.owned_by(1, "later-o")?),
            },
        ];
        assert_eq!(
            driver
                .ask(&connection, 3, &Write::new(terms, changes))
                .await?,
            Answer::Open,
            "the Write parks"
        );
        driver.release_homes();
        let (stream, frame) = connection.pushed("the Write's refusal").await?;
        let refused = error_in(&frame)?.context("an Error")?;
        assert_eq!(
            (stream, refused.code()),
            (3, ErrorCode::CrossPartition),
            "{}",
            refused.message()
        );
        Ok(())
    })
}

/// A Drop on a stream a Bind of kind 1 opened is refused naming kind 2, and ends that transaction.
#[test]
fn test_a_drop_on_a_transactions_stream_ends_it() -> TestResult {
    served(1, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let _bound = reply_of(
            driver
                .ask(&connection, 3, &bind(BindKind::Transaction, 1))
                .await?,
        )?;
        let drop = DropCommand::tenant_data(TENANT);
        let refused = refusal_of(driver.ask(&connection, 3, &drop).await?)?;
        assert_eq!(
            (refused.code(), refused.message()),
            (
                ErrorCode::Protocol,
                "Drop on a stream no Bind of kind 2 opened"
            )
        );
        let ended = refusal_of(driver.ask(&connection, 3, &Abort).await?)?;
        assert_eq!(
            ended.message(),
            "Abort on a stream no Bind of kind 1 opened"
        );
        Ok(())
    })
}

/// A Commit of a request below the last its session committed is refused by the commit step, its stream told so, the
/// refusal not run again.
#[test]
fn test_a_commit_its_round_refuses_is_told_on_its_stream() -> TestResult {
    served(1, async |driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let mut codes = Vec::new();
        for (stream, identity, request) in [(3, 1, 2), (5, 2, 1)] {
            let _bound = reply_of(
                driver
                    .ask(&connection, stream, &bind(BindKind::Transaction, identity))
                    .await?,
            )?;
            let held = reply_of(
                driver
                    .ask(
                        &connection,
                        stream,
                        &edge(&driver, 1, &format!("c{identity}"))?,
                    )
                    .await?,
            )?;
            assert_eq!(command_of(&held)?, Command::Held);
            let commit = Commit::new([4; 16], request, 0);
            assert_eq!(
                driver.ask(&connection, stream, &commit).await?,
                Answer::Open
            );
            let (on, frame) = connection.pushed("the Commit's outcome").await?;
            assert_eq!(on, stream);
            codes.push(error_in(&frame)?.map(|refused| refused.code()));
        }
        assert_eq!(codes, [None, Some(ErrorCode::RequestOutOfOrder)]);
        Ok(())
    })
}
