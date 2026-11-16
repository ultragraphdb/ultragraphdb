/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The registry and the marks as one Hegel state machine against a model written from `PROTOCOL.md` §11.5 and §13.23 to
//! §13.25: laws L3.1 to L3.14 of `notes/laws.md`. A property of its own holds L3.15 and L3.16 while a sync is held.

use std::collections::BTreeMap;

use compio::runtime::{JoinHandle, Runtime, spawn};
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_system::{SystemHandle, system_run};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{CheckKind, ErrorCode, SessionCheck};
use ultragraphdb_transactions_log::{GatedStorage, SimStorage, SyncGate, Wal, WalConfig};

use crate::utils::{CLUSTER, Clock, SimGroup, recovered, start_of};

/// `transaction_ms` of the machine; twice it in microseconds.
const T_MS: u64 = 50;
const HOLD_US: u64 = 2 * T_MS * 1_000;

/// The sessions the machine names, by number.
const SESSIONS: [u128; 6] = [1, 2, 3, 4, 5, 6];

/// A principal no command creates.
const UNKNOWN: u64 = 977;

/// What the model says a request on the registry comes to, as `PROTOCOL.md` §11.5 and §13.23 decide it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Said {
    Registered,
    Held,
    Unknown,
    Busy,
}

/// The registry and the marks as the specification states them.
#[derive(Debug, Clone, Default)]
struct Model {
    /// The sessions held, oldest first, each with its principal.
    sessions: Vec<(u128, u64)>,
    /// Each marked session's connection.
    marks: BTreeMap<u128, u64>,
    /// Each closed connection, by the instant it closed.
    closed: BTreeMap<u64, u64>,
    sessions_max: usize,
    now_us: u64,
    listened_us: u64,
}

impl Model {
    fn quiet(&self) -> bool {
        self.now_us < self.listened_us + HOLD_US
    }

    /// The marks whose connection closed twice `transaction_ms` ago or more end.
    fn expire(&mut self) {
        let now = self.now_us;
        let gone: Vec<u64> = self
            .closed
            .iter()
            .filter(|(_, at)| now >= **at + HOLD_US)
            .map(|(connection, _)| *connection)
            .collect();
        self.marks
            .retain(|_, connection| !gone.contains(connection));
    }

    fn holds(&self, session: u128) -> Option<u64> {
        self.sessions
            .iter()
            .find(|(held, _)| *held == session)
            .map(|(_, principal)| *principal)
    }

    /// A Session: Error 1 for a held identity, code 7 for an unknown principal, code 14 for a full registry holding no session
    /// it may evict, or one in the quiet time; else the oldest unmarked evicted and the session registered.
    fn session(&mut self, session: u128, principal: u64, known: &[u64]) -> Said {
        self.expire();
        if self.holds(session).is_some() {
            return Said::Held;
        }
        if !known.contains(&principal) {
            return Said::Unknown;
        }
        if self.sessions.len() >= self.sessions_max {
            if self.quiet() {
                return Said::Busy;
            }
            let Some(at) = self
                .sessions
                .iter()
                .position(|(held, _)| !self.marks.contains_key(held))
            else {
                return Said::Busy;
            };
            let _evicted = self.sessions.remove(at);
        }
        self.sessions.push((session, principal));
        Said::Registered
    }

    /// A check's outcome on `connection`: 12, 21, 14 or 1 in that order; kind 2 passing marks.
    fn check(&mut self, connection: u64, session: u128, principal: u64, marks: bool) -> u8 {
        let Some(owner) = self.holds(session) else {
            return 12;
        };
        if owner != principal {
            return 21;
        }
        if self.marks.contains_key(&session) || (!marks && self.quiet()) {
            return 14;
        }
        if marks {
            let _none = self.marks.insert(session, connection);
        }
        1
    }

    /// An Ended on `connection`: a mark of its own or of a closed connection ends.
    fn ended(&mut self, connection: u64, sessions: &[u128]) {
        self.expire();
        for session in sessions {
            let ends = self
                .marks
                .get(session)
                .is_some_and(|began| *began == connection || self.closed.contains_key(began));
            if ends {
                let _ended = self.marks.remove(session);
            }
        }
    }
}

/// The group the machine drives, on a runtime of its own.
struct Registry {
    runtime: Runtime,
    storage: SimStorage,
    clock: Clock,
    handle: Option<SystemHandle>,
    run: Option<JoinHandle<Result<(), RoleError>>>,
    model: Model,
    principals: Vec<u64>,
    open: Vec<u64>,
    sessions_max: u32,
}

impl Registry {
    fn new(sessions_max: u32) -> Option<Self> {
        let runtime = Runtime::new().ok()?;
        let storage = SimStorage::new(u64::from(sessions_max));
        let clock = Clock::new();
        let mut registry = Self {
            runtime,
            storage,
            clock,
            handle: None,
            run: None,
            model: Model {
                sessions_max: sessions_max as usize,
                ..Model::default()
            },
            principals: Vec::new(),
            open: Vec::new(),
            sessions_max,
        };
        registry.start()?;
        let handle = registry.handle.clone()?;
        let principals: Option<Vec<u64>> = registry.runtime.block_on(async {
            let mut ids = Vec::new();
            for name in ["alice", "bob"] {
                ids.push(handle.verifier(name.to_string()).await?.0);
            }
            Some(ids)
        });
        registry.principals = principals?;
        Some(registry)
    }

    /// The group recovered from the storage and run; it begins to listen now. The group begins to listen as its run begins
    /// (`notes/group-3.md`, *Found wanting* 2), so the run takes its first turn here, before the clock can move on.
    fn start(&mut self) -> Option<()> {
        let start = start_of(self.sessions_max, T_MS)?;
        let group: SimGroup =
            self.runtime
                .block_on(recovered(&self.storage, &start, &self.clock))?;
        let (handle, run) = system_run(group, self.clock.reading());
        self.run = Some(self.runtime.spawn(run));
        let _latest = self
            .runtime
            .block_on(async { handle.latest_close().await })
            .ok()?;
        self.handle = Some(handle);
        self.model.listened_us = self.clock.now_us();
        self.model.marks.clear();
        self.model.closed.clear();
        self.open.clear();
        Some(())
    }

    fn handle(&self) -> Option<SystemHandle> {
        self.handle.clone()
    }

    /// Every session the machine names checked as kind 1 on a connection of its own, outside the model: whether the group holds
    /// it, outcome 12 or not.
    fn held_by_group(&mut self) -> Option<Vec<bool>> {
        let handle = self.handle()?;
        self.runtime.block_on(async {
            let probe = handle.connect().await?;
            let mut held = Vec::new();
            for session in SESSIONS {
                for principal in [1_u64] {
                    let check =
                        SessionCheck::new(session.to_be_bytes(), principal, CheckKind::Asks);
                    let answered = handle.check(probe, vec![check]).await.ok()?;
                    held.push(answered.first()?.outcome().number() != 12);
                }
            }
            Some(held)
        })
    }
}

#[hegel::state_machine]
impl Registry {
    #[rule]
    fn session(&mut self, tc: TestCase) {
        let session = tc.draw(gs::sampled_from(SESSIONS.to_vec()));
        let mut principals = self.principals.clone();
        principals.push(UNKNOWN);
        let principal = tc.draw(gs::sampled_from(principals));
        let said = self.model.session(session, principal, &self.principals);
        let Some(handle) = self.handle() else { return };
        let answered = self
            .runtime
            .block_on(async { handle.session(session, principal).await });
        let got = match answered {
            Ok(()) => Some(Said::Registered),
            Err(refusal) if refusal.code() == ErrorCode::Protocol => Some(Said::Held),
            Err(refusal) if refusal.code() == ErrorCode::Refused => Some(Said::Unknown),
            Err(refusal) if refusal.code() == ErrorCode::SessionBusy => Some(Said::Busy),
            Err(_) => None,
        };
        assert_eq!(got, Some(said), "Session {session} of {principal}");
    }

    #[rule]
    fn check(&mut self, tc: TestCase) {
        tc.assume(!self.open.is_empty());
        let connection = tc.draw(gs::sampled_from(self.open.clone()));
        let count = tc.draw(gs::integers::<usize>().min_value(1).max_value(4));
        let mut sessions: Vec<u128> = (0..count)
            .map(|_| tc.draw(gs::sampled_from(SESSIONS.to_vec())))
            .collect();
        sessions.sort_unstable();
        sessions.dedup();
        let mut principals = self.principals.clone();
        principals.push(UNKNOWN);
        let checks: Vec<(u128, u64, bool)> = sessions
            .iter()
            .map(|session| {
                let principal = tc.draw(gs::sampled_from(principals.clone()));
                (*session, principal, tc.draw(gs::booleans()))
            })
            .collect();
        self.model.expire();
        let said: Vec<u8> = checks
            .iter()
            .map(|(session, principal, marks)| {
                self.model.check(connection, *session, *principal, *marks)
            })
            .collect();
        let sent: Vec<SessionCheck> = checks
            .iter()
            .map(|(session, principal, marks)| {
                let kind = if *marks {
                    CheckKind::Marks
                } else {
                    CheckKind::Asks
                };
                SessionCheck::new(session.to_be_bytes(), *principal, kind)
            })
            .collect();
        let Some(handle) = self.handle() else { return };
        let answered = self
            .runtime
            .block_on(async { handle.check(connection, sent.clone()).await });
        let got: Vec<u8> = answered
            .map(|checks| {
                checks
                    .iter()
                    .map(|check| check.outcome().number())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(got, said, "checks {checks:?} on {connection}");
    }

    #[rule]
    fn ended(&mut self, tc: TestCase) {
        tc.assume(!self.open.is_empty());
        let connection = tc.draw(gs::sampled_from(self.open.clone()));
        let mut sessions: Vec<u128> = (0..tc
            .draw(gs::integers::<usize>().min_value(1).max_value(4)))
            .map(|_| tc.draw(gs::sampled_from(SESSIONS.to_vec())))
            .collect();
        sessions.sort_unstable();
        sessions.dedup();
        self.model.ended(connection, &sessions);
        if let Some(handle) = self.handle() {
            handle.ended(connection, sessions);
            let taken = self.runtime.block_on(async { handle.latest_close().await });
            assert!(taken.is_ok(), "{taken:?}");
        }
    }

    #[rule]
    fn open(&mut self, _tc: TestCase) {
        let Some(handle) = self.handle() else { return };
        if let Some(connection) = self.runtime.block_on(async { handle.connect().await }) {
            self.open.push(connection);
        }
    }

    #[rule]
    fn close(&mut self, tc: TestCase) {
        tc.assume(!self.open.is_empty());
        let at = tc.draw(
            gs::integers::<usize>()
                .min_value(0)
                .max_value(self.open.len() - 1),
        );
        let connection = self.open.remove(at);
        let _first = self
            .model
            .closed
            .entry(connection)
            .or_insert(self.model.now_us);
        if let Some(handle) = self.handle() {
            handle.disconnect(connection);
            // The group takes the close before the clock moves on.
            let taken = self.runtime.block_on(async { handle.latest_close().await });
            assert!(taken.is_ok(), "{taken:?}");
        }
    }

    #[rule]
    fn pass(&mut self, tc: TestCase) {
        let span = tc.draw(gs::sampled_from(vec![
            1,
            T_MS * 1_000 - 1,
            T_MS * 1_000,
            HOLD_US - 1,
            HOLD_US,
            HOLD_US + 1,
        ]));
        self.clock.advance(span);
        self.model.now_us = self.clock.now_us();
    }

    #[rule]
    fn restart(&mut self, _tc: TestCase) {
        drop(self.handle.take());
        if let Some(run) = self.run.take() {
            let ended = self.runtime.block_on(run);
            assert!(
                matches!(ended, Ok(Ok(()))),
                "the group stopped cleanly: {ended:?}"
            );
        }
        assert!(self.start().is_some(), "the group recovered");
    }

    #[invariant]
    fn agrees(&mut self, _tc: TestCase) {
        self.model.expire();
        // L3.1: never more than `sessions_max`.
        assert!(self.model.sessions.len() <= self.model.sessions_max);
        let held_by_model: Vec<bool> = SESSIONS
            .iter()
            .map(|session| self.model.holds(*session).is_some())
            .collect();
        assert_eq!(self.held_by_group(), Some(held_by_model), "the registry");
    }
}

/// L3.1 to L3.14: any sequence of Session, Check, Ended, a connection's open and close, time passing and a restart keeps the
/// registry within `sessions_max`, evicts no marked session, and answers every request as the model of `PROTOCOL.md` §11.5 does.
#[hegel::test(test_cases = 40)]
fn test_the_registry_agrees_with_its_model(tc: TestCase) {
    let sessions_max = tc.draw(gs::integers::<u32>().min_value(1).max_value(4));
    let registry = Registry::new(sessions_max);
    assert!(registry.is_some(), "the group started");
    if let Some(registry) = registry {
        hegel::stateful::machine(registry).steps(30).run(tc);
    }
}

/// L3.15, L3.16: while the metadata log's sync of a registration is held, a Check sees the registration held and the session it
/// evicted not held, outcome 12, and is answered before the sync ends; the Session is answered only after.
#[hegel::test(test_cases = 20)]
fn test_a_check_reads_every_command_appended_while_a_sync_is_held(tc: TestCase) {
    let sessions_max = tc.draw(gs::integers::<u32>().min_value(1).max_value(4));
    let held = within_sync(sessions_max);
    let expected: Vec<u8> = (1..=sessions_max)
        .map(|session| if session == 1 { 12 } else { 1 })
        .chain([1])
        .collect();
    assert_eq!(held, Some((expected, false, true)));
}

/// A registry of `sessions_max` sessions 1 to `sessions_max`, its metadata sync held while one more registers: the outcomes of
/// checks of every session then, whether that Session was answered before the release, and whether after.
fn within_sync(sessions_max: u32) -> Option<(Vec<u8>, bool, bool)> {
    let runtime = Runtime::new().ok()?;
    runtime.block_on(async {
        let storage = SimStorage::new(7);
        let (metadata, sequencer) = (SyncGate::default(), SyncGate::default());
        let config = WalConfig::new(CLUSTER, false);
        let gated = |gate: &SyncGate| GatedStorage::new(storage.clone(), gate.clone());
        let (meta_log, _) = Wal::open(gated(&metadata), "/meta", config).await.ok()?;
        let (seq_log, _) = Wal::open(gated(&sequencer), "/seq", config).await.ok()?;
        let clock = Clock::new();
        let start = start_of(sessions_max, T_MS)?;
        let mut group = ultragraphdb_common_system::SystemGroup::recover(
            meta_log,
            seq_log,
            &start,
            None,
            clock.now(),
        )
        .await
        .ok()?;
        let verifier =
            ultragraphdb_protocol_scram::ScramVerifier::derive("pw", b"salt", 4_096).ok()?;
        group.create_principal_once("alice", verifier).await.ok()?;
        let (handle, run) = system_run(group, clock.reading());
        let run = spawn(run);
        // The group begins to listen at its run's first turn, which the verifier's answer follows; then its quiet time passes.
        let alice = handle.verifier("alice".to_string()).await?.0;
        clock.advance(HOLD_US);
        for session in 1..=sessions_max {
            handle.session(u128::from(session), alice).await.ok()?;
        }
        let connection = handle.connect().await?;
        metadata.hold();
        let newest = u128::from(sessions_max) + 1;
        let registering = {
            let handle = handle.clone();
            spawn(async move { handle.session(newest, alice).await })
        };
        metadata.until_waiting().await.ok()?;
        let checks = (1..=newest)
            .map(|session| SessionCheck::new(session.to_be_bytes(), alice, CheckKind::Asks))
            .collect();
        let outcomes: Vec<u8> = handle
            .check(connection, checks)
            .await
            .ok()?
            .iter()
            .map(|check| check.outcome().number())
            .collect();
        let early = registering.is_finished();
        metadata.release();
        let answered = registering.await.ok()?.is_ok();
        drop(handle);
        run.await.ok()?.ok()?;
        Some((outcomes, early, answered))
    })
}
