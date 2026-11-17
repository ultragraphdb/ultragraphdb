/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend over a system group a storage process hosts (design D2, D5): where it listens, what it keeps across its own
//! restart and a storage process's, how it waits for the group and what it answers while the group is gone.

use std::collections::BTreeSet;
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_scram::{ScramClient, system_nonce};
use ultragraphdb_protocol_transport::{
    Address, AuthenticationStep, BufferPool, ClientAuthentication, Framer,
};
use ultragraphdb_protocol_transport_compio::{Carrier, Connection};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ErrorCode, ErrorReplyView, Header, Hello, MAX_FRAME_SIZE,
    MAX_FRAME_SIZE_U32, Recall,
};
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, FRONTEND_OBSERVABILITY, SECRET, START_SECONDS, STORAGE_OBSERVABILITY,
    SYSTEM_ROLE_ADDRESS, exit_is, frontend_holding, http_status, members_at, until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::endpoint_steps::{ADMIN, PASSWORD, client_address};
use crate::processes::{
    ENDED, Roles, SECOND_ROLE_ADDRESS, admin, host_of, second_of, subject_of, write_of,
};

/// How long the frontend of a scenario that loses the system group for good waits for it.
const SHORT_DEADLINE_MS: &str = "800";

/// The authentications an address makes while the system group cannot be reached: past both the throttle's 50 and the ban's
/// further 20 at their defaults (`PROTOCOL.md` §11.9), so a count of any of them would delay or ban the address.
const OUTAGE_ATTEMPTS: usize = 80;

/// How often a step looks again at what it waits for.
const LOOK: Duration = Duration::from_millis(10);

/// The TCP addresses process `pid` listens at, read back from the operating system: Linux's `/proc`, and on macOS `lsof`, which
/// its base system holds.
fn listening(pid: u32) -> TestResult<BTreeSet<String>> {
    if cfg!(target_os = "linux") {
        return listening_in_proc(pid);
    }
    let listed = std::process::Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            "-a",
            "-p",
            &pid.to_string(),
            "-iTCP",
            "-sTCP:LISTEN",
            "-Fn",
        ])
        .output()?;
    let text = String::from_utf8(listed.stdout)?;
    // `-Fn` writes one field per line; a socket's name line begins with `n`.
    Ok(text
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        .map(str::to_string)
        .collect())
}

/// Linux: the listening sockets of `/proc/<pid>/net/tcp` whose inodes `pid`'s descriptors hold, as `address:port`.
fn listening_in_proc(pid: u32) -> TestResult<BTreeSet<String>> {
    let held: BTreeSet<String> = std::fs::read_dir(format!("/proc/{pid}/fd"))?
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .filter_map(|target| {
            let text = target.display().to_string();
            text.strip_prefix("socket:[")
                .and_then(|rest| rest.strip_suffix(']'))
                .map(str::to_string)
        })
        .collect();
    let table = std::fs::read_to_string(format!("/proc/{pid}/net/tcp"))?;
    let mut found = BTreeSet::new();
    // Columns: sl, local_address (hex IPv4 in host order : hex port), rem_address, st (0A is LISTEN), …, inode at 9.
    for line in table.lines().skip(1) {
        let columns: Vec<&str> = line.split_whitespace().collect();
        let (Some(local), Some(state), Some(inode)) =
            (columns.get(1), columns.get(3), columns.get(9))
        else {
            continue;
        };
        if *state != "0A" || !held.contains(*inode) {
            continue;
        }
        let (ip, port) = local.split_once(':').context("an address")?;
        let ip = u32::from_str_radix(ip, 16)?.to_le_bytes();
        let port = u16::from_str_radix(port, 16)?;
        let _new = found.insert(format!("{}.{}.{}.{}:{port}", ip[0], ip[1], ip[2], ip[3]));
    }
    Ok(found)
}

#[when("the frontend has started")]
fn frontend_started(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(1)?, true)?;
    roles.start_frontend(frontend_holding()?, true)?;
    w.seen = listening(roles.frontend_pid()?)?.into_iter().collect();
    w.processes = roles;
    Ok(())
}

#[then("it listens at its client address and its observability address alone")]
fn listens_alone(w: &mut Steps) {
    let expected: Vec<String> = [CLIENT_ADDRESS, FRONTEND_OBSERVABILITY]
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(w.seen, expected);
    w.processes = Roles::default();
}

#[when(
    "a client registers a session and commits request 1, the frontend is killed and restarted, and the client reconnects and commits request 2 through the same session"
)]
fn frontend_restarted(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(1)?, true)?;
    roles.start_frontend(frontend_holding()?, true)?;
    let subject = "http://ex.org/kept";
    let (number, first) = Runtime::new()?.block_on(async {
        let mut client = admin(1).await?;
        let session = client.register().await?;
        let committed = client
            .direct_write(write_of(subject, "http://ex.org/first"), session)
            .await?;
        compio::time::sleep(ENDED).await;
        TestResult::Ok((session.number(), committed.request()))
    })?;
    roles.restart_frontend()?;
    let second = Runtime::new()?.block_on(async {
        let mut client = admin(1).await?;
        // The session's next request is one above its last committed one (`PROTOCOL.md` §11.5).
        let resumed = client.resume_session(number, first + 1)?;
        Ok::<_, TestError>(
            client
                .direct_write(write_of(subject, "http://ex.org/second"), resumed)
                .await,
        )
    })?;
    w.seen = vec![
        first.to_string(),
        format!("{:?}", second.as_ref().map(|committed| committed.request())),
        roles.last_log("frontend")?,
    ];
    w.processes = roles;
    Ok(())
}

#[then("request 2 commits, and the frontend replayed no log and read no file of its own")]
fn request_2_committed(w: &mut Steps) -> TestResult {
    let [first, second, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    assert_eq!(first, "1", "request 1 was the session's first");
    assert_eq!(second, "Ok(2)", "{log}");
    // The frontend's settings name no directory of its own, and its restart says it recovered or replayed nothing.
    let named: Vec<String> = frontend_holding()?
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| name.ends_with("data_dir"))
        .collect();
    assert!(named.is_empty(), "{named:?}");
    for said in ["recover", "replay"] {
        assert!(!log.contains(said), "{said}: {log}");
    }
    w.processes = Roles::default();
    Ok(())
}

#[when("a storage process restarts at the same role address while the frontend runs")]
fn storage_restarted(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(2)?, true)?;
    roles.start_second(second_of()?)?;
    roles.start_frontend(frontend_holding()?, true)?;
    let subject = subject_of("http://ex.org/second", 2)?;
    let system = format!("tcp://{SYSTEM_ROLE_ADDRESS}");
    let before = Runtime::new()?.block_on(async {
        let mut client = admin(1).await?;
        Ok::<_, TestError>(
            client
                .direct_write(write_of(&subject, "http://ex.org/before"), None)
                .await,
        )
    })?;
    roles.restart_second()?;
    let members = members_at(&system)?;
    let after = Runtime::new()?.block_on(async {
        let mut client = admin(1).await?;
        Ok::<_, TestError>(
            client
                .direct_write(write_of(&subject, "http://ex.org/after"), None)
                .await,
        )
    })?;
    w.seen = vec![
        format!("{:?}", before.map(|_| ())),
        format!("{members:?}"),
        format!("{:?}", after.map(|_| ())),
        roles.last_log("frontend")?,
    ];
    w.processes = roles;
    Ok(())
}

#[then(
    "the frontend's connection to it closes, the frontend reads Members again and dials the address, and its next write to that partition is answered"
)]
fn dialled_again(w: &mut Steps) -> TestResult {
    let [before, members, after, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    assert_eq!(before, "Ok(())", "a write before the restart: {log}");
    // The restarted process joined again at its address, which Members names for partition 2.
    assert!(
        members.contains(&format!("2, \"tcp://{SECOND_ROLE_ADDRESS}\"")),
        "{members}"
    );
    assert_eq!(after, "Ok(())", "the next write: {log}");
    w.processes = Roles::default();
    Ok(())
}

/// Polls the `/ready` of `frontend` and `storage` until the frontend's answers: whether storage's answered when it did, and
/// whether the frontend's `/health` answered while its `/ready` did not yet.
fn readiness_order() -> TestResult<(bool, bool)> {
    let ready = |address: &str| http_status(address, "/ready").is_ok_and(|status| status == 200);
    let mut healthy_first = false;
    for _ in 0..START_SECONDS * 100 {
        if ready(FRONTEND_OBSERVABILITY) {
            // Readiness only rises, so storage's `/ready` read after the frontend's says whether it answered before.
            return Ok((ready(STORAGE_OBSERVABILITY), healthy_first));
        }
        healthy_first |=
            http_status(FRONTEND_OBSERVABILITY, "/health").is_ok_and(|status| status == 200);
        std::thread::sleep(LOOK);
    }
    Err(TestError::new("the frontend's /ready never answered"))
}

#[when("the frontend starts while the hosting storage process still recovers")]
fn frontend_while_recovering(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(1)?, true)?;
    roles.start_frontend(frontend_holding()?, true)?;
    // Rounds in the host's partition log, which a restart replays before the partition joins and storage is ready.
    Runtime::new()?.block_on(async {
        let mut client = admin(1).await?;
        for at in 0..200 {
            let _committed = client
                .direct_write(
                    write_of("http://ex.org/rounds", &format!("http://ex.org/o{at}")),
                    None,
                )
                .await?;
        }
        TestResult::Ok(())
    })?;
    let (host, settings) = roles.host.take().context("the host")?;
    let frontend = roles.frontend_pid()?;
    let _host_gone = roles.kill(host)?;
    let _frontend_gone = roles.kill(frontend)?;
    let frontend_settings = roles.frontend.take().context("the frontend")?.1;
    roles.start_host(settings, false)?;
    roles.start_frontend_unwaited(frontend_settings)?;
    let storage_ready_now =
        http_status(STORAGE_OBSERVABILITY, "/ready").is_ok_and(|status| status == 200);
    let (storage_first, healthy_first) = readiness_order()?;
    w.seen = vec![
        storage_ready_now.to_string(),
        storage_first.to_string(),
        healthy_first.to_string(),
        roles.last_log("storage-host")?,
    ];
    w.processes = roles;
    Ok(())
}

#[then("the frontend's `/health` answers, and its `/ready` answers only after storage's")]
fn ready_after_storage(w: &mut Steps) -> TestResult {
    let [storage_ready_then, storage_first, healthy_first, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    // The premise: storage was still recovering when the frontend's `/health` answered.
    assert_eq!(
        storage_ready_then, "false",
        "storage was ready already: {log}"
    );
    assert_eq!(healthy_first, "true", "the frontend's /health first");
    assert_eq!(storage_first, "true", "storage's /ready first: {log}");
    w.processes = Roles::default();
    Ok(())
}

#[when(
    "the frontend starts while no process listens at `cluster.system_address`, and the hosting storage process starts within the deadline"
)]
fn frontend_before_storage(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_frontend(frontend_holding()?, false)?;
    let unready = http_status(FRONTEND_OBSERVABILITY, "/ready")?;
    // The frontend dials meanwhile; the host starts well within the default deadline of 60 s.
    std::thread::sleep(Duration::from_millis(500));
    let still = http_status(FRONTEND_OBSERVABILITY, "/ready")?;
    roles.start_host(host_of(1)?, false)?;
    let (storage_first, _healthy) = readiness_order()?;
    w.seen = vec![
        unready.to_string(),
        still.to_string(),
        storage_first.to_string(),
        roles.last_log("frontend")?,
    ];
    w.processes = roles;
    Ok(())
}

#[then("the frontend dials until it is answered, and its `/ready` answers after storage's")]
fn waited_for_storage(w: &mut Steps) -> TestResult {
    let [unready, still, storage_first, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    assert_eq!(unready, "503", "{log}");
    assert_eq!(still, "503", "{log}");
    assert_eq!(storage_first, "true", "{log}");
    w.processes = Roles::default();
    Ok(())
}

#[when("the hosting storage process is killed and does not return within `join_deadline_ms`")]
fn host_gone(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(1)?, true)?;
    let settings = with(
        frontend_holding()?,
        "cluster.join_deadline_ms",
        SHORT_DEADLINE_MS,
    );
    roles.start_frontend(settings, true)?;
    let _killed = roles.kill(roles.host_pid()?)?;
    let frontend = roles.frontend_pid()?;
    w.exited = Some(
        roles
            .util()?
            .wait_for_exit(frontend, Duration::from_secs(START_SECONDS))?,
    );
    w.log = roles.last_log("frontend")?;
    w.processes = roles;
    Ok(())
}

#[then(
    "the frontend shuts down through its ordinary path and exits with code 69 naming the system group's address"
)]
fn frontend_ended(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains(SYSTEM_ROLE_ADDRESS), "{}", w.log);
    assert!(w.log.contains("stopping role=frontend"), "{}", w.log);
    w.processes = Roles::default();
    Ok(())
}

/// The code a client's error carries: a refusal's, or the refusal that left a commit in doubt.
fn code_of(error: &ClientError) -> Option<ErrorCode> {
    match &error.0 {
        ClientErrorEnum::Refused { code, .. } | ClientErrorEnum::Closed { code, .. } => Some(*code),
        ClientErrorEnum::InDoubt {
            cause: Some(cause), ..
        } => code_of(cause),
        _ => None,
    }
}

/// What partition 2 answers a Recall of `request` of `session` with: the command of its answer, and an Error's code.
fn recalled(session: u128, request: u64) -> TestResult<(Command, Option<ErrorCode>)> {
    Runtime::new()?.block_on(async {
        let address = Address::parse(&format!("tcp://{SECOND_ROLE_ADDRESS}"))?;
        let mut role = RoleClient::dial(&address, CLUSTER, "frontend", SECRET).await?;
        role.send_only(&Recall::new(2, session.to_be_bytes(), request))
            .await?;
        let answer = role.next_frame().await?;
        let view = answer.view()?;
        let code = (view.command() == Command::Error)
            .then(|| ErrorReplyView::from_frame(&view).map(|refusal| refusal.code()))
            .transpose()?;
        Ok((view.command(), code))
    })
}

#[when("the frontend's connection to the group is closed and a client commits through its session")]
fn commit_in_outage(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(2)?, true)?;
    roles.start_second(second_of()?)?;
    roles.start_frontend(frontend_holding()?, true)?;
    let subject = subject_of("http://ex.org/outage", 2)?;
    let runtime = Runtime::new()?;
    let (mut client, session, first) = runtime.block_on(async {
        let mut client = admin(1).await?;
        let session = client.register().await?;
        let committed = client
            .direct_write(write_of(&subject, "http://ex.org/first"), session)
            .await?;
        compio::time::sleep(ENDED).await;
        TestResult::Ok((client, session, committed.request()))
    })?;
    let (host, settings) = roles.host.take().context("the host")?;
    let _killed = roles.kill(host)?;
    let refused = runtime.block_on(async {
        client
            .direct_write(write_of(&subject, "http://ex.org/during"), session)
            .await
    });
    // The partition's table, asked while the group is gone: whether the refused request reached it.
    let at_partition = recalled(session.number(), first + 1)?;
    roles.start_host(settings, true)?;
    until_answering(FRONTEND_OBSERVABILITY, "/ready")?;
    let resent = runtime.block_on(async { client.resolve(session).await });
    w.seen = vec![
        format!("{:?}", refused.as_ref().err().and_then(code_of)),
        format!("{at_partition:?}"),
        format!("{:?}", resent.map(|committed| committed.request())),
        (first + 1).to_string(),
        roles.last_log("frontend")?,
    ];
    w.processes = roles;
    Ok(())
}

#[then(
    "the Commit is answered code 17 and reaches no partition, and the same Commit succeeds once the group answers again"
)]
fn refused_not_lost(w: &mut Steps) -> TestResult {
    let [refused, at_partition, resent, request, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    assert_eq!(
        refused,
        &format!("{:?}", Some(ErrorCode::Unavailable)),
        "{log}"
    );
    assert_eq!(
        at_partition,
        &format!("{:?}", (Command::Error, Some(ErrorCode::NotCommitted))),
        "the partition's table holds no request of it"
    );
    assert_eq!(resent, &format!("Ok({request})"), "{log}");
    w.processes = Roles::default();
    Ok(())
}

/// One authentication as the administrator on a connection of its own: `None` once it authenticates, else the stream and the
/// code of the Error that answered it.
async fn authenticated_once() -> TestResult<Option<(u64, ErrorCode)>> {
    let carrier = Carrier::connect(&client_address()?).await?;
    let mut connection = Connection::new(carrier, Framer::new(CLUSTER, MAX_FRAME_SIZE));
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let hello = Hello::new(1, 1, 1, MAX_FRAME_SIZE_U32, "client");
    let _welcome = connection.hello(&pool, &hello).await?;
    let nonce = system_nonce()?;
    let mut exchange = ClientAuthentication::new(ScramClient::new(ADMIN, PASSWORD, &nonce)?);
    let mut frame = exchange
        .start()?
        .frame(Header::new(CLUSTER, Command::Authenticate))?;
    // The Hello was frame 1.
    let mut sent: u128 = 1;
    // An exchange is two client messages; each turn sends one or ends.
    loop {
        sent += 1;
        Header::stamp_sequence(&mut frame, sent)?;
        let _written = connection.write_frame(frame).await?;
        let answer = compio::time::timeout(Duration::from_secs(10), connection.read_frame(&pool))
            .await
            .map_err(|_| TestError::new("no frame came"))??;
        let view = answer.view()?;
        if view.command() == Command::Error {
            let refusal = ErrorReplyView::from_frame(&view)?;
            return Ok(Some((view.header().stream(), refusal.code())));
        }
        match exchange.receive(&view)? {
            AuthenticationStep::Send(message) => {
                frame = message.frame(Header::new(CLUSTER, Command::Authenticate))?;
            }
            AuthenticationStep::Done { .. } => return Ok(None),
        }
    }
}

#[when(
    "one address authenticates 80 times while the system group cannot be reached, and once after it answers again"
)]
fn outage_authentications(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(1)?, true)?;
    roles.start_frontend(frontend_holding()?, true)?;
    let (host, settings) = roles.host.take().context("the host")?;
    let _killed = roles.kill(host)?;
    let runtime = Runtime::new()?;
    let during = runtime.block_on(async {
        let mut answers = Vec::new();
        for _ in 0..OUTAGE_ATTEMPTS {
            answers.push(authenticated_once().await?);
        }
        TestResult::Ok(answers)
    })?;
    roles.start_host(settings, true)?;
    // The group answers again once each endpoint core's link has dialled it: until then a core answers 17 as before, counting
    // nothing. The first answer that is not 17 is the scenario's last authentication.
    let after = runtime.block_on(async {
        for _ in 0..START_SECONDS * 10 {
            match authenticated_once().await? {
                Some((0, ErrorCode::Unavailable)) => {
                    compio::time::sleep(Duration::from_millis(100)).await;
                }
                other => return Ok(other),
            }
        }
        Err(TestError::new(
            "the frontend never relayed an exchange again",
        ))
    })?;
    w.seen = vec![
        format!("{during:?}"),
        format!("{after:?}"),
        roles.last_log("frontend")?,
    ];
    w.processes = roles;
    Ok(())
}

#[then("each of the 80 is answered Error 17 and counted as no failure, and the last authenticates")]
fn outage_bans_nothing(w: &mut Steps) -> TestResult {
    let [during, after, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    let expected = vec![Some((0u64, ErrorCode::Unavailable)); OUTAGE_ATTEMPTS];
    assert_eq!(during, &format!("{expected:?}"), "{log}");
    // Counted, 80 failures would have banned the address past 70 (`PROTOCOL.md` §11.9), closing its connection as accepted.
    assert_eq!(after, "None", "{log}");
    w.processes = Roles::default();
    Ok(())
}

#[when(
    "the frontend is ready, and the hosting storage process is killed and started again within `join_deadline_ms`"
)]
fn group_lost_and_back(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(1)?, true)?;
    roles.start_frontend(frontend_holding()?, true)?;
    let ready_before = http_status(FRONTEND_OBSERVABILITY, "/ready")?;
    let (host, settings) = roles.host.take().context("the host")?;
    let _killed = roles.kill(host)?;
    // The frontend's `/ready` once its link has found the connection closed, and its `/health` then; the default deadline,
    // 60 s, leaves the frontend running throughout.
    let mut withdrawn = None;
    for _ in 0..START_SECONDS * 100 {
        let ready = http_status(FRONTEND_OBSERVABILITY, "/ready")?;
        if ready != 200 {
            withdrawn = Some((ready, http_status(FRONTEND_OBSERVABILITY, "/health")?));
            break;
        }
        std::thread::sleep(LOOK);
    }
    let (unready, healthy) = withdrawn.context("the frontend's /ready withdrawn")?;
    roles.start_host(settings, true)?;
    until_answering(FRONTEND_OBSERVABILITY, "/ready")?;
    let ready_after = http_status(FRONTEND_OBSERVABILITY, "/ready")?;
    w.seen = vec![
        ready_before.to_string(),
        unready.to_string(),
        healthy.to_string(),
        ready_after.to_string(),
        roles.last_log("frontend")?,
    ];
    w.processes = roles;
    Ok(())
}

#[then(
    "the frontend's `/ready` answers 503 once its connection to the system group has closed while its `/health` answers 200, and its `/ready` answers 200 again once the group answers it, Members lists every partition and one answers"
)]
fn unready_while_lost(w: &mut Steps) -> TestResult {
    let [before, unready, healthy, after, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    assert_eq!(before, "200", "ready before the loss: {log}");
    assert_eq!(unready, "503", "{log}");
    assert_eq!(healthy, "200", "{log}");
    assert_eq!(after, "200", "{log}");
    w.processes = Roles::default();
    Ok(())
}
