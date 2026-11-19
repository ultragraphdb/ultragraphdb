/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The roles over a system group a storage process hosts, by its `[system]` table, as built binaries (design D2, D5): where
//! the client endpoint listens, what it keeps across its own restart and a storage process's, how the roles wait for the
//! group, what the endpoint answers while the group is gone, and how compute joins a group again (system-group, "A lost
//! system group is waited for until the deadline").

use std::collections::BTreeSet;
use std::process::ExitStatus;
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
    MAX_FRAME_SIZE_U32, Recall, RoleKind,
};
use ultragraphdb_tests::{
    CLUSTER, COMPUTE_BINARY, SECRET, START_SECONDS, STORAGE_BINARY, answering,
    compute_observability, compute_on_one_host, exit_is, fresh_data, frontend_holding,
    frontend_observability, holding, http_status, members_at, role, scratch, storage_holding,
    storage_observability, system_role_address, until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult, loopback};

use crate::endpoint_steps::{ADMIN, PASSWORD, client_address};
use crate::processes::{
    ENDED, Roles, admin, host_of, second_of, second_role_address, signal, subject_of, write_of,
};
use crate::{Steps, services};

/// Where the hosting storage process's system group listens, as an offset above the target's port base: an address of its
/// own beside its partitions'.
const SYSTEM_ROLE_OFFSET: u16 = 4;

/// How long the roles of a scenario that loses the system group for good wait for it.
const SHORT_DEADLINE_MS: &str = "800";

/// The authentications an address makes while the system group cannot be reached: past both the throttle's 50 and the ban's
/// further 20 at their defaults (`PROTOCOL.md` §11.9), so a count of any of them would delay or ban the address.
const OUTAGE_ATTEMPTS: usize = 80;

/// How often a step looks again at what it waits for.
const LOOK: Duration = Duration::from_millis(10);

/// The address of the system group a scenario's storage process hosts at [`SYSTEM_ROLE_OFFSET`].
pub(crate) fn own_system_address() -> TestResult<String> {
    loopback(SYSTEM_ROLE_OFFSET)
}

/// The deadline of the compute process whose group restarts: compute asks the group whether it lists compute every half of
/// it, a minute here, so only its connection's close lists it again within the [`START_SECONDS`] the scenario waits.
const LONG_DEADLINE_MS: u64 = 120_000;

/// `settings` dialling the system group at `system`.
fn dialling(settings: Vec<(String, String)>, system: &str) -> Vec<(String, String)> {
    with(settings, "cluster.system_address", system)
}

/// A storage process of one partition hosting the system group at `system`, its logs in a directory of their own.
fn hosting(system: &str) -> TestResult<Vec<(String, String)>> {
    let data = fresh_data();
    let tables = [
        ("cluster.partition_count", "1"),
        ("system.role_address", system),
        ("system.data_dir", data.as_str()),
        ("sequencer.epoch_ms_min", "1"),
        ("sequencer.epoch_ms_max", "20"),
    ];
    Ok(tables.into_iter().fold(
        dialling(storage_holding()?, system),
        |settings, (name, value)| with(settings, name, value),
    ))
}

/// Whether the system group at `system` lists a compute process within [`START_SECONDS`].
fn compute_listed(system: &str) -> TestResult<bool> {
    for _ in 0..START_SECONDS * 20 {
        if members_at(system)?
            .iter()
            .any(|(kind, _, _)| *kind == RoleKind::Compute)
        {
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(false)
}

#[when("the hosting storage process is killed and started again within `join_deadline_ms`")]
fn hosting_storage_restarted(w: &mut Steps) -> TestResult {
    let util = services()?;
    let system = format!("tcp://{}", own_system_address()?);
    let settings = hosting(&system)?;
    let storage_log = scratch("storage-restarted");
    let storage_ready = storage_observability()?;
    let hosting_storage = || {
        role(
            STORAGE_BINARY,
            &settings,
            &storage_log,
            answering(&storage_ready, "/ready"),
        )
    };
    let storage = util.start_service_from_config(hosting_storage()?)?;
    let log = scratch("compute-restarted-group.log");
    let _compute: u32 = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &with(
            dialling(holding(compute_on_one_host()?, SECRET)?, &system),
            "cluster.join_deadline_ms",
            &LONG_DEADLINE_MS.to_string(),
        ),
        &log,
        answering(&compute_observability()?, "/ready"),
    )?)?;
    assert!(compute_listed(&system)?, "compute joined at its start");
    signal(storage, "KILL")?;
    let _killed = util.wait_for_exit(storage, Duration::from_secs(START_SECONDS))?;
    let _again: u32 = util.start_service_from_config(hosting_storage()?)?;
    w.outcome = Some(compute_listed(&system)?);
    w.log = std::fs::read_to_string(&log)?;
    w.roles = Some(util);
    Ok(())
}

#[then(
    "compute dials the group as its connection closes and joins it again on the connection the group answers, so the group's Members lists compute again as soon as the group is back"
)]
fn joined_again_at_once(w: &mut Steps) -> TestResult {
    let (waited_ms, next_ask_ms) = (START_SECONDS * 1_000, LONG_DEADLINE_MS / 2);
    if waited_ms >= next_ask_ms {
        return Err(TestError::new(format!(
            "the step waits {waited_ms} ms, past compute's next ask at {next_ask_ms} ms"
        )));
    }
    assert_eq!(w.outcome, Some(true), "{}", w.log);
    w.roles = None;
    Ok(())
}

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
fn listens_alone(w: &mut Steps) -> TestResult {
    let expected: Vec<String> = [
        ultragraphdb_tests::client_address()?,
        frontend_observability()?,
    ]
    .into_iter()
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect();
    assert_eq!(w.seen, expected);
    w.processes = Roles::default();
    Ok(())
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
    let system = format!("tcp://{}", system_role_address()?);
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
        members.contains(&format!("2, \"tcp://{}\"", second_role_address()?)),
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
    let (frontend, storage) = (frontend_observability()?, storage_observability()?);
    let mut healthy_first = false;
    for _ in 0..START_SECONDS * 100 {
        if ready(&frontend) {
            // Readiness only rises, so storage's `/ready` read after the frontend's says whether it answered before.
            return Ok((ready(&storage), healthy_first));
        }
        healthy_first |= http_status(&frontend, "/health").is_ok_and(|status| status == 200);
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
        http_status(&storage_observability()?, "/ready").is_ok_and(|status| status == 200);
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
    let unready = http_status(&frontend_observability()?, "/ready")?;
    // The frontend dials meanwhile; the host starts well within the default deadline of 60 s.
    std::thread::sleep(Duration::from_millis(500));
    let still = http_status(&frontend_observability()?, "/ready")?;
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

/// The code a client's error carries: a refusal's, or the refusal that left a commit in doubt.
fn code_of(error: &ClientError) -> Option<ErrorCode> {
    match &error.0 {
        ClientErrorEnum::Refused { code, .. } | ClientErrorEnum::Closed { code, .. } => Some(*code),
        ClientErrorEnum::InDoubt {
            cause: Some(cause), ..
        } => code_of(cause),
        ClientErrorEnum::Transport { .. }
        | ClientErrorEnum::Protocol { .. }
        | ClientErrorEnum::TimedOut
        | ClientErrorEnum::ConnectionLost { .. }
        | ClientErrorEnum::NotConnected
        | ClientErrorEnum::Request { .. }
        | ClientErrorEnum::ReplyMismatch { .. }
        | ClientErrorEnum::OutcomeUnknown { .. }
        | ClientErrorEnum::InDoubt { .. }
        | ClientErrorEnum::NotInDoubt { .. }
        | ClientErrorEnum::SessionBusy { .. }
        | ClientErrorEnum::UnknownSession { .. }
        | ClientErrorEnum::ReadOnly { .. }
        | ClientErrorEnum::StreamFinished { .. }
        | ClientErrorEnum::StreamBusy { .. }
        | ClientErrorEnum::StreamLimit { .. }
        | ClientErrorEnum::BufferLimit { .. }
        | ClientErrorEnum::TransactionMismatch { .. } => None,
    }
}

/// What partition 2 answers a Recall of `request` of `session` with: the command of its answer, and an Error's code.
fn recalled(session: u128, request: u64) -> TestResult<(Command, Option<ErrorCode>)> {
    Runtime::new()?.block_on(async {
        let address = Address::parse(&format!("tcp://{}", second_role_address()?))?;
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
    until_answering(&frontend_observability()?, "/ready")?;
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
    let ready_before = http_status(&frontend_observability()?, "/ready")?;
    let (host, settings) = roles.host.take().context("the host")?;
    let _killed = roles.kill(host)?;
    // The frontend's `/ready` once its link has found the connection closed, and its `/health` then; the default deadline,
    // 60 s, leaves the frontend running throughout.
    let mut withdrawn = None;
    let frontend = frontend_observability()?;
    for _ in 0..START_SECONDS * 100 {
        let ready = http_status(&frontend, "/ready")?;
        if ready != 200 {
            withdrawn = Some((ready, http_status(&frontend, "/health")?));
            break;
        }
        std::thread::sleep(LOOK);
    }
    let (unready, healthy) = withdrawn.context("the frontend's /ready withdrawn")?;
    roles.start_host(settings, true)?;
    until_answering(&frontend_observability()?, "/ready")?;
    let ready_after = http_status(&frontend_observability()?, "/ready")?;
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

#[when("the hosting storage process is killed and does not return within `join_deadline_ms`")]
fn hosting_storage_killed(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(1)?, true)?;
    // Both roles that depend on the group, each with a short deadline, so each scenario's Then judges its own.
    let deadline = |settings| with(settings, "cluster.join_deadline_ms", SHORT_DEADLINE_MS);
    roles.start_frontend(deadline(frontend_holding()?), true)?;
    let compute = roles.start_compute(deadline(holding(compute_on_one_host()?, SECRET)?))?;
    let _killed = roles.kill(roles.host_pid()?)?;
    for (name, pid) in [("frontend", roles.frontend_pid()?), ("compute", compute)] {
        let exited = roles
            .util()?
            .wait_for_exit(pid, Duration::from_secs(START_SECONDS))?;
        w.dependants
            .push((name, Some(exited), roles.last_log(name)?));
    }
    w.processes = roles;
    Ok(())
}

/// How the role `name` ended when its system group was lost, and its log.
fn dependant<'a>(w: &'a Steps, name: &str) -> TestResult<(Option<ExitStatus>, &'a str)> {
    w.dependants
        .iter()
        .find(|(role, _, _)| *role == name)
        .map(|(_, exited, log)| (*exited, log.as_str()))
        .context("the role's end")
}

/// That the role `name` stopped through its ordinary path, the one SIGTERM takes, which logs the role's stop before it
/// drains its work, and exited with code 69 naming the system group's address.
fn ended_naming_the_group(w: &mut Steps, name: &str) -> TestResult {
    let (exited, log) = dependant(w, name)?;
    exit_is(exited, log, 69)?;
    assert!(log.contains(&system_role_address()?), "{log}");
    assert!(log.contains(&format!("stopping role={name}")), "{log}");
    w.processes = Roles::default();
    Ok(())
}

#[then(
    "compute shuts down through its ordinary path and exits with code 69 naming the system group's address"
)]
fn compute_ended(w: &mut Steps) -> TestResult {
    ended_naming_the_group(w, "compute")
}

#[then(
    "the frontend shuts down through its ordinary path and exits with code 69 naming the system group's address"
)]
fn frontend_ended(w: &mut Steps) -> TestResult {
    ended_naming_the_group(w, "frontend")
}
