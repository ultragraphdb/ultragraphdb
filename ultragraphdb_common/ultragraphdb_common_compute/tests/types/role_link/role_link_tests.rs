/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A core's one connection to a partition, carrying the Scans and Closed of the reads the core executes beside its Bind
//! streams (design D2): the corner cases C4.1 to C4.22 of `notes/corner-cases.md`. The link alone is driven through the
//! probe of `apart_link`; the reads, through a core over the links, against the played partitions.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::net::TcpListener;
use ultragraphdb_common_compute::{
    DoubleFrame, LinkProbe, ReadExecutor, ReadRequest, RoleDouble, RoleLinks, Told,
    partition_answer, scan_answer,
};
use ultragraphdb_common_role::{Answer, OpenRequests};
use ultragraphdb_core_keys::Graph;
use ultragraphdb_protocol_transport::Endpoint;
use ultragraphdb_protocol_wire::{
    BodyView, Closed, ClosedPartition, Command, Commit, DEFAULT_INITIAL_CREDIT, ErrorCode,
    ErrorReply, ErrorReplyView, Held, MAX_FRAME_SIZE, MAX_FRAME_SIZE_U32, ReadLevel,
};
use ultragraphdb_store_sim::LogCapture;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::apart::apart_cluster::{
    ApartCluster, CLUSTER, even_map, links_listing, played_role_cluster, run, times,
};
use crate::apart::apart_executor::{executed, pattern_of, read_frame, snapshot};
use crate::apart::apart_link::{
    LOOK, PARTITION, Probed, QUIET, WAIT, bind, closes, entries_on, frame_on, open_bind, opened,
    probed, scan, scan_from, taken, told, told_quietly,
};
use crate::apart::apart_partition::{EPOCH, Script, VERSION};
use crate::apart::apart_reads::{
    P, TENANT, TWO_BY_TWO, answer_on, hold_lists, next_frame, of_p, read_of, rows_listed, send_read,
};
use crate::apart::apart_session::SessionRead;

/// C4.1: a Scan whose frame cannot be built, its key past the largest frame, is refused by its frame's rule, and no stream is
/// registered for it.
#[test]
fn test_a_scan_that_cannot_be_built_registers_no_stream() -> TestResult {
    run(async {
        let Probed { mut probe, .. } = probed(false).await?;
        let huge = vec![b'k'; MAX_FRAME_SIZE];
        let refused = probe
            .open_read(&scan_from(&huge))
            .err()
            .context("the Scan's refusal")?;
        assert_eq!(refused.code(), ErrorCode::Protocol, "{}", refused.message());
        assert_eq!(probe.streams_open(), 0);
        assert!(!probe.is_closed());
        Ok(())
    })
}

/// C4.2: a Scan sent on a link that closed beneath it registers no stream, and the read takes code 17.
#[test]
fn test_a_scan_on_a_link_closed_beneath_it_registers_no_stream() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let bound = open_bind(&mut probe)?;
        let _bind = taken(&double, Command::Bind, bound).await?;
        double.drop_connections();
        assert_eq!(told(&mut probe).await?, (bound, Told::Closed));
        let refused = probe
            .open_read_on_held(&scan())
            .err()
            .context("the Scan's refusal")?;
        assert_eq!(
            refused.code(),
            ErrorCode::Unavailable,
            "{}",
            refused.message()
        );
        assert_eq!(probe.streams_open(), 0);
        Ok(())
    })
}

/// C4.3 and C4.4: a write that fails, a Bind stream and two Scans open on the link, marks the link closed at once and tells each
/// of the three streams so once; the next request dials again, is sent on the new connection and answered there.
#[test]
fn test_a_failed_write_closes_the_link_and_the_next_request_dials_again() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let bound = open_bind(&mut probe)?;
        let first = opened(probe.open_read(&scan()))?;
        let second = opened(probe.open_read(&scan()))?;
        let _scan = taken(&double, Command::Scan, second).await?;
        probe.send_unwritable(bound);
        let mut expected = vec![bound, first, second];
        expected.sort_unstable();
        assert_eq!(closes(&mut probe, 3).await?, expected);
        assert!(probe.is_closed());
        assert_eq!(
            told_quietly(&mut probe).await,
            None,
            "each stream told once"
        );
        let again = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Scan, again).await?;
        request.answer_with(entries_on(again, true)?);
        assert_eq!(
            told(&mut probe).await?,
            (again, Told::Frame(Command::Entries))
        );
        assert_eq!(double.accepted(), 2, "the link was dialled again");
        assert!(!probe.is_closed());
        Ok(())
    })
}

/// C4.5: with the tap granting the partition one frame more than compute did, the frame past the stream's credit is refused as
/// past credit, naming the stream: the link closes, no Error is sent, and both reads it carried are told the close.
#[test]
fn test_a_frame_past_credit_closes_the_link_and_ends_every_read() -> TestResult {
    let capture = LogCapture::new();
    let (seen, errors_sent) = capture.capture(|| {
        run(async {
            let Probed {
                double,
                tap,
                mut probe,
                ..
            } = probed(true).await?;
            tap.context("the tap")?.overgrant(1);
            let other = opened(probe.open_read(&scan()))?;
            let spent = opened(probe.open_read(&scan()))?;
            let request = taken(&double, Command::Scan, spent).await?;
            let credit = DEFAULT_INITIAL_CREDIT;
            for _ in 0..=credit {
                request.answer_with(entries_on(spent, false)?);
            }
            let mut seen = Vec::new();
            for _ in 0..credit {
                seen.push(told(&mut probe).await?);
            }
            let mut expected = vec![other, spent];
            expected.sort_unstable();
            assert_eq!(closes(&mut probe, 2).await?, expected);
            compio::time::sleep(QUIET).await;
            TestResult::Ok((seen, double.count(Command::Error)))
        })
    })?;
    let credit = usize::try_from(DEFAULT_INITIAL_CREDIT)?;
    assert_eq!(seen.len(), credit);
    assert!(
        seen.iter()
            .all(|(_, what)| *what == Told::Frame(Command::Entries)),
        "{seen:?}"
    );
    assert_eq!(errors_sent, 0, "the side that dialled sends no Error");
    let refusals: Vec<String> = capture
        .events()
        .into_iter()
        .filter_map(|event| event.field("refused").map(str::to_string))
        .collect();
    assert!(
        refusals.iter().any(|why| why.contains("credit")),
        "the close names the credit: {refusals:?}"
    );
    Ok(())
}

/// C4.6 and C4.17: a Scan's stream starts with the partition's `initial_credit`: Entries frames up to exactly it are each
/// taken, the link open; a grant lets one more come, and the partition is sent one Credit for it.
#[test]
fn test_entries_up_to_the_credit_keep_the_link() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let stream = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Scan, stream).await?;
        for _ in 0..DEFAULT_INITIAL_CREDIT {
            request.answer_with(entries_on(stream, false)?);
        }
        for _ in 0..DEFAULT_INITIAL_CREDIT {
            assert_eq!(
                told(&mut probe).await?,
                (stream, Told::Frame(Command::Entries))
            );
        }
        assert!(!probe.is_closed());
        probe.grant(stream);
        request.answer_with(entries_on(stream, true)?);
        assert_eq!(
            told(&mut probe).await?,
            (stream, Told::Frame(Command::Entries))
        );
        assert!(!probe.is_closed());
        assert_eq!(probe.streams_open(), 0, "the last frame ended the stream");
        assert_eq!(double.count(Command::Credit), 1);
        Ok(())
    })
}

/// C4.7: a frame on a stream whose reader is gone is checked whole: one that holds is dropped and the link stays open; one
/// whose chunk's checksum fails is refused, which closes the link.
#[test]
fn test_a_frame_of_a_gone_reader_is_checked_whole() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let gone = opened(probe.open_read(&scan()))?;
        let carrier = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Scan, carrier).await?;
        probe.cancel(gone);
        request.answer_with(entries_on(gone, false)?);
        assert_eq!(
            told_quietly(&mut probe).await,
            None,
            "a late frame is told to no one"
        );
        assert!(!probe.is_closed(), "a late frame that holds keeps the link");
        let mut broken = entries_on(gone, false)?;
        let last = broken.last_mut().context("a frame's last byte")?;
        *last ^= 0xFF;
        request.answer_with(broken);
        assert_eq!(told(&mut probe).await?, (carrier, Told::Closed));
        assert!(probe.is_closed());
        Ok(())
    })
}

/// C4.9: an Entries frame on a stream that carries a Bind and no Scan answers no request of it, and closes the link as a Bind's
/// stray reply does: each stream it carried is told the close.
#[test]
fn test_entries_on_a_bind_stream_close_the_link() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let bound = open_bind(&mut probe)?;
        let reading = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Bind, bound).await?;
        let _scan = taken(&double, Command::Scan, reading).await?;
        request.answer_with(entries_on(bound, false)?);
        let mut expected = vec![bound, reading];
        expected.sort_unstable();
        assert_eq!(closes(&mut probe, 2).await?, expected);
        assert!(probe.is_closed());
        Ok(())
    })
}

/// C4.10: a Held on a Scan's stream answers no request of it, and closes the link.
#[test]
fn test_a_held_on_a_scans_stream_closes_the_link() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let reading = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Scan, reading).await?;
        request.answer_with(frame_on(reading, &Held, false)?);
        assert_eq!(told(&mut probe).await?, (reading, Told::Closed));
        assert!(probe.is_closed());
        Ok(())
    })
}

/// C4.11: a frame on a stream at or above the next the link will open answers no request, and closes the link.
#[test]
fn test_a_frame_ahead_of_the_streams_closes_the_link() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let reading = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Scan, reading).await?;
        let ahead = probe.next_stream().context("the next stream")?;
        request.answer_with(entries_on(ahead, false)?);
        assert_eq!(told(&mut probe).await?, (reading, Told::Closed));
        assert!(probe.is_closed());
        Ok(())
    })
}

/// C4.12: an Error on a Scan's stream ends that read with its code, the link open; an Error on stream 0 is the link's refusal
/// and closes it.
#[test]
fn test_an_error_ends_its_read_and_one_on_stream_0_closes_the_link() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let refused = opened(probe.open_read(&scan()))?;
        let carrier = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Scan, refused).await?;
        let refusal = ErrorReply::new(ErrorCode::TimedOut, "a played refusal");
        request.answer_with(frame_on(refused, &refusal, true)?);
        assert_eq!(
            told(&mut probe).await?,
            (refused, Told::Refused(ErrorCode::TimedOut))
        );
        assert!(!probe.is_closed());
        let carried = taken(&double, Command::Scan, carrier).await?;
        let connection = ErrorReply::new(ErrorCode::Protocol, "a played connection error");
        carried.answer_with(frame_on(0, &connection, false)?);
        assert_eq!(told(&mut probe).await?, (carrier, Told::Closed));
        assert!(probe.is_closed());
        Ok(())
    })
}

/// C4.13: a Scan cancelled while its Entries are on their way takes them as late frames: checked, dropped, told to no one, the
/// link open.
#[test]
fn test_entries_after_a_cancel_are_late() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let cancelled = opened(probe.open_read(&scan()))?;
        let carrier = opened(probe.open_read(&scan()))?;
        let carried = taken(&double, Command::Scan, carrier).await?;
        let _scan = taken(&double, Command::Scan, cancelled).await?;
        carried.answer_with(entries_on(cancelled, false)?);
        probe.cancel(cancelled);
        assert_eq!(told_quietly(&mut probe).await, None);
        assert!(!probe.is_closed());
        carried.answer_with(entries_on(carrier, true)?);
        assert_eq!(
            told(&mut probe).await?,
            (carrier, Told::Frame(Command::Entries))
        );
        Ok(())
    })
}

/// C4.14: one link carrying a transaction's Commit and another connection's Scan, the Scan answered past its credit: the link
/// closes, and the Commit's stream and the read are each told the close, the Commit in doubt.
#[test]
fn test_a_scan_past_credit_ends_a_commit_on_the_same_link() -> TestResult {
    run(async {
        let Probed {
            double,
            tap,
            mut probe,
            ..
        } = probed(true).await?;
        tap.context("the tap")?.overgrant(1);
        let bound = open_bind(&mut probe)?;
        let bind_request = taken(&double, Command::Bind, bound).await?;
        bind_request.answer_with(frame_on(bound, &bind().with_read_version(VERSION), false)?);
        assert_eq!(told(&mut probe).await?, (bound, Told::Frame(Command::Bind)));
        probe.send(bound, frame_on(bound, &Commit::new([1; 16], 1, 0), false)?);
        let _commit = taken(&double, Command::Commit, bound).await?;
        let reading = opened(probe.open_read(&scan()))?;
        let request = taken(&double, Command::Scan, reading).await?;
        for _ in 0..=DEFAULT_INITIAL_CREDIT {
            request.answer_with(entries_on(reading, false)?);
        }
        let mut closed = Vec::new();
        // The read's frames within its credit come first, then the close of both streams.
        while closed.len() < 2 {
            let (stream, what) = told(&mut probe).await?;
            match what {
                Told::Closed => closed.push(stream),
                Told::Frame(Command::Entries) if stream == reading => {}
                other => {
                    return Err(TestError::new(format!("stream {stream} told {other:?}")));
                }
            }
        }
        closed.sort_unstable();
        let mut expected = vec![bound, reading];
        expected.sort_unstable();
        assert_eq!(closed, expected);
        Ok(())
    })
}

/// C4.18: a Closed request is answered by Closed on its stream, which ends the stream.
#[test]
fn test_a_closed_request_is_answered_on_its_stream() -> TestResult {
    run(async {
        let Probed {
            double, mut probe, ..
        } = probed(false).await?;
        let asked = opened(probe.open_read(&Closed::request(&[PARTITION])))?;
        let request = taken(&double, Command::Closed, asked).await?;
        let reply = Closed::new(vec![ClosedPartition::new(PARTITION, EPOCH)]);
        request.answer_with(frame_on(asked, &reply, true)?);
        assert_eq!(
            told(&mut probe).await?,
            (asked, Told::Frame(Command::Closed))
        );
        assert_eq!(probe.streams_open(), 0);
        assert!(!probe.is_closed());
        Ok(())
    })
}

/// C4.19: a link dialled at a peer that accepts and says nothing fails past its dial bound as a closed connection does: the
/// request it carried is told the close, and the log names the address and the bound.
#[test]
fn test_a_dial_past_its_bound_closes_the_link() -> TestResult {
    const DIAL_MS: u32 = 300;
    let capture = LogCapture::new();
    let (address, closed) = capture.capture(|| {
        run(async {
            let silent = TcpListener::bind("127.0.0.1:0").await?;
            let address = format!("tcp://{}", silent.local_addr()?);
            let held: Rc<Cell<Option<compio::net::TcpStream>>> = Rc::default();
            let holding = Rc::clone(&held);
            compio::runtime::spawn(async move {
                if let Ok((peer, _)) = silent.accept().await {
                    holding.set(Some(peer));
                }
                std::future::pending::<()>().await;
            })
            .detach();
            let links: RoleLinks = links_listing(
                &played_role_cluster()?,
                even_map(1)?,
                &[(PARTITION, address.clone())],
            )
            .with_dial_ms(DIAL_MS);
            let mut probe = LinkProbe::new(links, PARTITION);
            let reading = opened(probe.open_read(&scan()))?;
            let closed = told(&mut probe).await? == (reading, Told::Closed);
            TestResult::Ok((address, closed))
        })
    })?;
    assert!(closed, "the request was told the close");
    let named = capture.events().into_iter().any(|event| {
        let text = format!("{event:?}");
        text.contains(address.trim_start_matches("tcp://")) && text.contains(&DIAL_MS.to_string())
    });
    assert!(named, "the log names {address} and {DIAL_MS} ms");
    Ok(())
}

/// The lists `partition` holds in [`C4_8_ORDER`]'s test: one subject of its own and one object.
fn lists_of(partition: u32) -> (String, String) {
    (
        format!("http://ex.org/s{partition}"),
        format!("http://ex.org/o{partition}"),
    )
}

/// The order three partitions bind their ports in, which is not the order their numbers sort in.
const C4_8_ORDER: [u32; 3] = [3, 1, 2];

/// C4.8: three partitions bound to ports in the order 3, 1, 2: each Scan reaches the partition its subject names, by number,
/// and the read's rows come partition by partition in ascending order of partition (`PROTOCOL.md` §9.1).
#[test]
fn test_each_scan_reaches_its_partition_whatever_the_order_bound() -> TestResult {
    run(async {
        let cluster = ApartCluster::start_in(&C4_8_ORDER, false).await?;
        let mut expected = Vec::new();
        for partition in 1..=3 {
            let (subject, object) = lists_of(partition);
            let lists: &[(&str, &[&str])] = &[(subject.as_str(), &[object.as_str()])];
            let _p = hold_lists(
                cluster.partition(partition)?,
                (Graph::Default, P),
                lists,
                u64::from(partition) * 100,
            )?;
            expected.extend(rows_listed(lists));
        }
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(1))));
        let pattern = pattern_of(&read_frame((false, None, Some(P)), 0, Vec::new())?)?;
        let (answer, _sizes) =
            executed(&mut executor, snapshot(pattern, MAX_FRAME_SIZE_U32)).await?;
        assert_eq!(answer.whole()?, expected.as_slice());
        for partition in 1..=3 {
            let scans = cluster.partition(partition)?.scans();
            assert!(!scans.is_empty(), "partition {partition} was asked");
        }
        Ok(())
    })
}

/// C4.15: a partition leaving a read's Scan unanswered past its wait and twice `scan_stall_ms` is hung: compute closes the
/// link, the read is refused with code 17 and the Bind stream of another transaction on the link is told the close.
#[test]
fn test_a_hung_partition_ends_a_bind_stream_on_the_same_link() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let _p = hold_lists(partition, (Graph::Default, P), TWO_BY_TWO, 100)?;
        partition.script(Vec::new(), Script::Silent, 0);
        let links = cluster.links();
        let core = cluster.core_over(links.clone(), times(Duration::from_millis(100)));
        let mut probe = LinkProbe::new(links, 1);
        let bound = open_bind(&mut probe)?;
        assert_eq!(told(&mut probe).await?, (bound, Told::Frame(Command::Bind)));
        let mut client = cluster.session_on(core);
        let answer = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        let (code, _, message) = answer.refused.context("the read's refusal")?;
        assert_eq!(code, ErrorCode::Unavailable, "{message}");
        assert_eq!(told(&mut probe).await?, (bound, Told::Closed));
        Ok(())
    })
}

/// C4.16: a read-write transaction's Bind and its read's Scans leave on one link: the partition takes one connection, the
/// Bind first.
#[test]
fn test_a_transactions_bind_and_reads_share_one_link() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let _p = hold_lists(partition, (Graph::Default, P), TWO_BY_TWO, 100)?;
        let links = cluster.links();
        let core = cluster.core_over(links.clone(), times(Duration::from_secs(1)));
        let mut probe = LinkProbe::new(links, 1);
        let bound = open_bind(&mut probe)?;
        assert_eq!(told(&mut probe).await?, (bound, Told::Frame(Command::Bind)));
        let mut client = cluster.session_on(core);
        let answer = answer_on(&mut client, &of_p(0, Vec::new())).await?;
        assert_eq!(answer.whole()?, rows_listed(TWO_BY_TWO).as_slice());
        assert_eq!(partition.connections(), 1);
        let requests = partition.requests();
        assert_eq!(requests.first(), Some(&(1, Command::Bind)));
        assert!(
            requests.iter().all(|(connection, _)| *connection == 1)
                && requests
                    .iter()
                    .any(|(_, command)| *command == Command::Scan),
            "{requests:?}"
        );
        Ok(())
    })
}

/// C4.20: two cores each hold a link to one partition; one link closing, by a frame compute refuses, leaves the other core's
/// reads going on, and the first core's next read dials again.
#[test]
fn test_one_cores_link_closing_leaves_the_others() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(1).await?;
        let partition = cluster.partition(1)?;
        let p = hold_lists(partition, (Graph::Default, P), TWO_BY_TWO, 100)?;
        let lists = ultragraphdb_core_keys::key_prefix(
            ultragraphdb_core_keys::Keyspace::Forward,
            false,
            TENANT,
            Some(Graph::Default),
            Some(p),
        );
        let stall = times(Duration::from_secs(1));
        let mut first = cluster.session(stall, OpenRequests::default());
        let mut second = cluster.session(stall, OpenRequests::default());
        let whole = rows_listed(TWO_BY_TWO);
        assert_eq!(
            answer_on(&mut second, &of_p(0, Vec::new()))
                .await?
                .whole()?,
            whole.as_slice()
        );
        partition.script(lists, Script::BreakOrder, 1);
        let broken = answer_on(&mut first, &of_p(0, Vec::new())).await?;
        assert_eq!(
            broken.refused.context("a refusal")?.0,
            ErrorCode::Unavailable
        );
        assert_eq!(
            answer_on(&mut second, &of_p(0, Vec::new()))
                .await?
                .whole()?,
            whole.as_slice()
        );
        assert_eq!(partition.connections(), 2, "a link per core");
        assert_eq!(
            answer_on(&mut first, &of_p(0, Vec::new())).await?.whole()?,
            whole.as_slice()
        );
        assert_eq!(partition.connections(), 3, "the closed link dialled again");
        Ok(())
    })
}

/// C4.21: a storage process restarted at its address while a read is open: the link closes, the read is refused with code 17,
/// the closed link is found, so Members is read again, and the next read dials the address and is answered.
#[test]
fn test_a_storage_process_restarted_under_a_read_is_dialled_again() -> TestResult {
    run(async {
        let cluster = played_role_cluster()?;
        let holding = Rc::new(Cell::new(true));
        let held = Rc::clone(&holding);
        let double = RoleDouble::start(&cluster, Endpoint::Partition, move |frame| {
            if frame.command() == Command::Scan && held.get() {
                return Answer::Open;
            }
            if frame.command() == Command::Scan {
                return scan_answer(frame, CLUSTER, VERSION);
            }
            partition_answer(frame, CLUSTER, VERSION)
        })
        .await?;
        let links = links_listing(&cluster, even_map(1)?, &[(1, double.address().to_string())]);
        let played = ApartCluster::start(1).await?;
        let core = played.core_over(links.clone(), times(Duration::from_secs(1)));
        let mut client = played.session_on(core);
        let read: SessionRead = read_of((false, Some("http://ex.org/s"), Some(P)), 0, Vec::new());
        let stream = send_read(&mut client, &read)?;
        let _scan = taken_any(&double, Command::Scan).await?;
        double.drop_connections();
        let refused = next_frame(&mut client).await?;
        let view = refused.view()?;
        assert_eq!(view.header().stream(), stream);
        assert_eq!(view.command(), Command::Error);
        assert_eq!(
            ErrorReplyView::from_frame(&view)?.code(),
            ErrorCode::Unavailable
        );
        holding.set(false);
        let answer = answer_on(&mut client, &read).await?;
        assert!(answer.refused.is_none(), "{:?}", answer.refused);
        compio::time::timeout(WAIT, links.closed_found())
            .await
            .map_err(|_| TestError::new("the closed link was not found"))?;
        assert_eq!(double.accepted(), 2);
        Ok(())
    })
}

/// Waits until `double` took any request of `command`.
async fn taken_any(double: &RoleDouble, command: Command) -> TestResult<DoubleFrame> {
    let waited = async {
        // Each look waits a little; the wait bounds them.
        loop {
            if let Some(frame) = double.frames(command).into_iter().next() {
                return frame;
            }
            compio::time::sleep(LOOK).await;
        }
    };
    compio::time::timeout(WAIT, waited)
        .await
        .map_err(|_| TestError::new(format!("the double took no {command:?}")))
}

/// C4.22: a monotonic-fast read over two partitions sends a Closed on each partition's link before that link carries the read's
/// Scans: each partition takes one connection, a Closed and then the Scans.
#[test]
fn test_a_monotonic_fast_read_asks_closed_on_each_link_before_its_scans() -> TestResult {
    run(async {
        let cluster = ApartCluster::start(2).await?;
        let mut expected = Vec::new();
        for partition in 1..=2 {
            let (subject, object) = lists_of(partition);
            let lists: &[(&str, &[&str])] = &[(subject.as_str(), &[object.as_str()])];
            let _p = hold_lists(
                cluster.partition(partition)?,
                (Graph::Default, P),
                lists,
                u64::from(partition) * 100,
            )?;
            expected.extend(rows_listed(lists));
        }
        let mut executor = ReadExecutor::new(cluster.core(times(Duration::from_secs(1))));
        let pattern = pattern_of(&read_frame((false, None, Some(P)), 0, Vec::new())?)?;
        let read = ReadRequest::new(pattern, TENANT, true, ReadLevel::MonotonicFast)
            .with_epoch(1)
            .with_principal(1)
            .with_deadline_ms(10_000)
            .with_max_frame(MAX_FRAME_SIZE_U32);
        let (answer, _sizes) = executed(&mut executor, read).await?;
        assert_eq!(answer.whole()?, expected.as_slice());
        for partition in 1..=2 {
            let played = cluster.partition(partition)?;
            assert_eq!(played.connections(), 1, "partition {partition}");
            let commands: Vec<Command> = played
                .requests()
                .into_iter()
                .map(|(_, command)| command)
                .collect();
            assert_eq!(commands.first(), Some(&Command::Closed), "{commands:?}");
            assert!(commands.contains(&Command::Scan), "{commands:?}");
        }
        Ok(())
    })
}
