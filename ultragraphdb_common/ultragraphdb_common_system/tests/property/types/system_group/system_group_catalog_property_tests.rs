/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog as one Hegel state machine against a model written from `PROTOCOL.md` §11.13, §13.7 and §13.27: laws L3.17 to
//! L3.28 of `notes/laws.md`. A property of its own holds L3.29, the cap.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use compio::runtime::{JoinHandle, Runtime};
use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_common_system::{DropStep, ReportAsked, SystemHandle, system_run};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_wire::{Declared, DeclaredPredicate, ErrorCode, ErrorReply};
use ultragraphdb_transactions_log::SimStorage;

use crate::utils::{Clock, recovered, start_of};

/// The predicates the machine declares.
const PREDICATES: [&str; 2] = ["http://ex.org/p", "http://ex.org/q"];

/// A predicate no Declare names.
const NEVER: &str = "http://ex.org/never";

/// The predicates the machine records default entries of.
const RECORDED: [&str; 3] = ["http://ex.org/p", "http://ex.org/r", "http://ex.org/s"];

/// A tenant the group does not hold.
const UNKNOWN_TENANT: u64 = 9;

/// An epoch's length past its longest: moving the clock by it makes a close due.
const EPOCH_US: u64 = 10_000;

/// Where a version stands against its check (`PROTOCOL.md` §11.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    Settled,
    Checking,
    Withdrawn,
}

/// One version of the model: what it declares, each predicate single-valued or not, its effective epoch and its standing.
#[derive(Debug, Clone)]
struct Version {
    declares: BTreeMap<&'static str, bool>,
    effective: Option<u64>,
    standing: Standing,
}

/// What the model says a Declare comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Committed as this version.
    Committed(u64),
    /// Refused with this code and `detail`.
    Refused(ErrorCode, u64),
}

/// The catalog of tenant 0 and its default-entry records as the specification states them.
#[derive(Debug, Clone, Default)]
struct Model {
    versions: Vec<Version>,
    defaults: BTreeSet<&'static str>,
    latest: u64,
}

impl Model {
    fn current(&self) -> u64 {
        self.versions.len() as u64 // lossless: a `usize` is at most 64 bits
    }

    fn declares(&self, version: u64) -> BTreeMap<&'static str, bool> {
        version
            .checked_sub(1)
            .and_then(|at| self.versions.get(at as usize)) // lossless: the versions held are few
            .map(|held| held.declares.clone())
            .unwrap_or_default()
    }

    /// A Declare of `kind` 1 of `declared`, each a predicate and whether it is single-valued, built on `built_on`.
    fn declare(&mut self, built_on: u64, declared: &[(&'static str, bool)]) -> Outcome {
        if built_on != self.current() {
            return Outcome::Refused(ErrorCode::Conflict, 5);
        }
        if self
            .versions
            .iter()
            .any(|held| held.standing == Standing::Checking)
        {
            return Outcome::Refused(ErrorCode::Conflict, 5);
        }
        let held = self.declares(built_on);
        let narrows = declared
            .iter()
            .any(|(predicate, single)| *single && !held.get(predicate).copied().unwrap_or(false));
        let widens = declared.iter().position(|(predicate, single)| {
            !*single && held.get(predicate).copied().unwrap_or(false)
        });
        if let (true, Some(index)) = (narrows, widens) {
            return Outcome::Refused(ErrorCode::Refused, index as u64); // lossless, as above
        }
        let mut next = held;
        for (predicate, single) in declared {
            let _before = next.insert(*predicate, *single);
        }
        let standing = if narrows {
            Standing::Checking
        } else {
            Standing::Settled
        };
        self.versions.push(Version {
            declares: next,
            effective: None,
            standing,
        });
        Outcome::Committed(self.current())
    }

    /// A Declare of `kind` 2 for `step`: the version it builds, 0 for none; a Drop of everything removes the records.
    fn drop_version(&mut self, step: &DropStep) -> u64 {
        let held = self.declares(self.current());
        let next = match step {
            DropStep::Predicate(predicate) => {
                let mut next = held.clone();
                next.retain(|declared, _| *declared != predicate.as_str());
                next
            }
            DropStep::Everything => {
                self.defaults.clear();
                BTreeMap::new()
            }
            DropStep::Kept => held.clone(),
        };
        if next.len() == held.len() {
            return 0;
        }
        self.versions.push(Version {
            declares: next,
            effective: None,
            standing: Standing::Settled,
        });
        self.current()
    }

    /// A close of epoch `epoch`: every version without an epoch takes the one after it.
    fn close(&mut self, epoch: u64) {
        self.latest = epoch;
        for version in &mut self.versions {
            if version.effective.is_none() {
                version.effective = Some(epoch + 1);
            }
        }
    }

    /// The one partition's report on `version`: passed settles it; refused withdraws it with a version restoring what it
    /// changed, built on the current one. A report of a version not being checked changes nothing.
    fn report(&mut self, version: u64, refused: bool) {
        let at = version.saturating_sub(1) as usize; // lossless, as above
        let checking = self
            .versions
            .get(at)
            .is_some_and(|held| held.standing == Standing::Checking && held.effective.is_some());
        if !checking {
            return;
        }
        if !refused {
            if let Some(held) = self.versions.get_mut(at) {
                held.standing = Standing::Settled;
            }
            return;
        }
        let before = self.declares(version - 1);
        let withdrawn = self.declares(version);
        let mut next = self.declares(self.current());
        for (predicate, single) in &withdrawn {
            let previous = before.get(predicate);
            if previous == Some(single) {
                continue;
            }
            if next.get(predicate) == Some(single) {
                match previous {
                    Some(previous) => {
                        let _restored = next.insert(*predicate, *previous);
                    }
                    None => {
                        let _gone = next.remove(predicate);
                    }
                }
            }
        }
        if let Some(held) = self.versions.get_mut(at) {
            held.standing = Standing::Withdrawn;
        }
        self.versions.push(Version {
            declares: next,
            effective: None,
            standing: Standing::Settled,
        });
    }

    /// What a Catalog request of `version` 0 naming `epoch` reads: the version, or the refusal's code and `detail`.
    fn catalog_at(&self, epoch: u64) -> Result<u64, (ErrorCode, u64)> {
        if epoch > self.latest {
            return Err((ErrorCode::Protocol, 0));
        }
        if epoch == 0 {
            return match self.current() {
                0 => Err((ErrorCode::Refused, 0)),
                current => Ok(current),
            };
        }
        let mut read = None;
        for (at, version) in self.versions.iter().enumerate() {
            if version.effective.is_none_or(|effective| effective > epoch) {
                break;
            }
            match version.standing {
                Standing::Settled => read = Some(at as u64 + 1), // lossless, as above
                Standing::Checking => break,
                Standing::Withdrawn => {}
            }
        }
        read.ok_or((ErrorCode::Refused, 0))
    }
}

/// A refusal's code and `detail`, or the version a Declare committed.
fn outcome_of(answer: Result<(u64, u64), ErrorReply>) -> Outcome {
    match answer {
        Ok((version, _)) => Outcome::Committed(version),
        Err(refusal) => Outcome::Refused(refusal.code(), refusal.detail()),
    }
}

/// A Declare's ask in flight, answered with the version and its commit version.
type Waiting = JoinHandle<Result<(u64, u64), ErrorReply>>;

/// The group the machine drives, on a runtime of its own.
struct Catalog {
    runtime: Runtime,
    storage: SimStorage,
    clock: Clock,
    handle: Option<SystemHandle>,
    run: Option<JoinHandle<Result<(), RoleError>>>,
    model: Model,
    /// The Declares of `kind` 1 committed and not yet answered: the version, and their asks.
    waiting: Vec<(u64, Waiting)>,
}

impl Catalog {
    fn new() -> Option<Self> {
        let mut catalog = Self {
            runtime: Runtime::new().ok()?,
            storage: SimStorage::new(17),
            clock: Clock::new(),
            handle: None,
            run: None,
            model: Model::default(),
            waiting: Vec::new(),
        };
        catalog.start()?;
        Some(catalog)
    }

    fn start(&mut self) -> Option<()> {
        let start = start_of(8, 50)?;
        let group = self
            .runtime
            .block_on(recovered(&self.storage, &start, &self.clock))?;
        let (handle, run) = system_run(group, self.clock.reading());
        self.run = Some(self.runtime.spawn(run));
        self.handle = Some(handle);
        Some(())
    }

    fn handle(&self) -> Option<SystemHandle> {
        self.handle.clone()
    }

    /// Asks a lease, whose answer waits for the metadata log's sync: every command appended before it is durable after it.
    fn synced(&self) {
        if let Some(handle) = self.handle() {
            let leased = self.runtime.block_on(async { handle.lease(1, 1, 1).await });
            assert!(leased.is_ok(), "{leased:?}");
        }
    }

    /// The Declares answered so far, each against the model: committed as their version, at the epoch the model assigned it; once
    /// the group `stopped`, each still waiting refused as unavailable.
    fn answered(&mut self, stopped: bool) {
        let waiting = std::mem::take(&mut self.waiting);
        for (version, task) in waiting {
            if !task.is_finished() {
                self.waiting.push((version, task));
                continue;
            }
            let answer = self.runtime.block_on(task);
            let at = version.saturating_sub(1) as usize; // lossless, as above
            let model = self.model.versions.get(at).cloned();
            match (answer, model) {
                (Ok(Ok((answered, commit_version))), Some(held)) => {
                    // L3.21, L3.22: answered once its epoch is assigned and its check passed.
                    assert_eq!(answered, version);
                    assert_eq!(held.standing, Standing::Settled);
                    assert_eq!(Some(commit_version >> 22), held.effective);
                    assert_eq!(commit_version & ((1 << 22) - 1), 0);
                }
                (Ok(Err(refusal)), Some(_))
                    if stopped && refusal.code() == ErrorCode::Unavailable => {}
                (Ok(Err(refusal)), Some(held)) => {
                    // L3.22: a refusing report refuses it with code 7, its version withdrawn.
                    assert_eq!(refusal.code(), ErrorCode::Refused, "{refusal:?}");
                    assert_eq!(held.standing, Standing::Withdrawn);
                }
                (other, held) => assert!(held.is_none() && other.is_err(), "{other:?} {held:?}"),
            }
        }
    }
}

#[hegel::state_machine]
impl Catalog {
    #[rule]
    fn declare(&mut self, tc: TestCase) {
        let current = self.model.current();
        let built_on = tc.draw(gs::sampled_from(vec![
            current,
            current.saturating_sub(1),
            current + 1,
            0,
        ]));
        let first = tc.draw(gs::sampled_from(PREDICATES.to_vec()));
        let mut declared = vec![(first, tc.draw(gs::booleans()))];
        if tc.draw(gs::booleans()) {
            let second = if first == PREDICATES[0] {
                PREDICATES[1]
            } else {
                PREDICATES[0]
            };
            declared.push((second, tc.draw(gs::booleans())));
        }
        let unknown = tc.draw(gs::booleans()) && tc.draw(gs::booleans());
        let asked: Vec<Declared> = declared
            .iter()
            .map(|(predicate, single)| {
                Declared::Predicate(DeclaredPredicate::new(*predicate).with_single(*single))
            })
            .collect();
        let Some(handle) = self.handle() else { return };
        if unknown {
            // L3.28: a tenant the group does not hold.
            let refused = self.runtime.block_on(async {
                handle
                    .declare_version(UNKNOWN_TENANT, built_on, asked)
                    .await
            });
            assert_eq!(
                outcome_of(refused),
                Outcome::Refused(ErrorCode::Refused, UNKNOWN_TENANT)
            );
            return;
        }
        let said = self.model.declare(built_on, &declared);
        let task = self
            .runtime
            .spawn(async move { handle.declare_version(0, built_on, asked).await });
        match said {
            Outcome::Committed(version) => {
                // L3.25: an asker gone changes nothing the group does.
                if tc.draw(gs::booleans()) {
                    self.waiting.push((version, task));
                } else {
                    self.runtime
                        .block_on(compio::time::sleep(Duration::from_millis(1)));
                    drop(task);
                }
                self.synced();
            }
            Outcome::Refused(..) => {
                // L3.18, L3.19, L3.23: refused at once, the current version kept.
                let answer = self
                    .runtime
                    .block_on(compio::time::timeout(Duration::from_secs(2), task));
                let got = answer.ok().and_then(Result::ok).map(outcome_of);
                assert_eq!(got, Some(said));
            }
        }
    }

    #[rule]
    fn drop_version(&mut self, tc: TestCase) {
        let step = tc.draw(gs::sampled_from(vec![
            DropStep::Predicate(PREDICATES[0].to_string()),
            DropStep::Predicate(PREDICATES[1].to_string()),
            DropStep::Predicate(NEVER.to_string()),
            DropStep::Kept,
            DropStep::Everything,
        ]));
        let said = self.model.drop_version(&step);
        let Some(handle) = self.handle() else { return };
        let got = self
            .runtime
            .block_on(async { handle.drop_version(0, step.clone()).await });
        // L3.24, L3.26: a Drop that changes nothing builds none; one of everything removes the records.
        assert_eq!(got.ok(), Some(said), "{step:?}");
    }

    #[rule]
    fn default_entry(&mut self, tc: TestCase) {
        let predicate = tc.draw(gs::sampled_from(RECORDED.to_vec()));
        let found = !self.model.defaults.insert(predicate);
        let Some(handle) = self.handle() else { return };
        let got = self
            .runtime
            .block_on(async { handle.record_default(0, predicate.to_string()).await });
        assert_eq!(got.ok(), Some(found), "{predicate}");
    }

    #[rule]
    fn close(&mut self, _tc: TestCase) {
        let Some(handle) = self.handle() else { return };
        let after = self.model.latest;
        self.clock.advance(EPOCH_US);
        let closed = self
            .runtime
            .block_on(async { handle.next_close(after).await })
            .map(|close| close.epoch().get());
        assert_eq!(closed.ok(), Some(after + 1));
        self.model.close(after + 1);
        // The close is learned, and what waited on it answered, before the next ask is taken.
        self.synced();
        self.answered(false);
    }

    #[rule]
    fn report(&mut self, tc: TestCase) {
        let checking: Vec<u64> = (1..=self.model.current())
            .filter(|version| {
                self.model
                    .versions
                    .get((*version - 1) as usize) // lossless, as above
                    .is_some_and(|held| {
                        held.standing == Standing::Checking && held.effective.is_some()
                    })
            })
            .collect();
        tc.assume(!checking.is_empty());
        let version = tc.draw(gs::sampled_from(checking));
        let refused = tc.draw(gs::booleans());
        self.model.report(version, refused);
        let Some(handle) = self.handle() else { return };
        let at_fault = refused.then(|| (PREDICATES[0].to_string(), "\"x\"".to_string()));
        let report = ReportAsked::new(Tenant::new(0), version, 1, at_fault);
        let got = self.runtime.block_on(async { handle.report(report).await });
        assert!(got.is_ok(), "{got:?}");
        self.synced();
        self.answered(false);
    }

    #[rule]
    fn restart(&mut self, _tc: TestCase) {
        // The host killed: its run dropped where it stands, every Declare still waiting losing its answer with it.
        drop(self.handle.take());
        drop(self.run.take());
        let waiting = std::mem::take(&mut self.waiting);
        for (version, task) in waiting {
            self.runtime
                .block_on(compio::time::sleep(Duration::from_millis(1)));
            self.waiting.push((version, task));
        }
        self.answered(true);
        assert!(
            self.waiting.is_empty(),
            "every ask answered once the group stopped"
        );
        assert!(self.start().is_some(), "the group recovered");
    }

    #[rule]
    fn catalog_at(&mut self, tc: TestCase) {
        let epoch = tc.draw(
            gs::integers::<u64>()
                .min_value(0)
                .max_value(self.model.latest + 1),
        );
        let said = self.model.catalog_at(epoch);
        let Some(handle) = self.handle() else { return };
        let got = self
            .runtime
            .block_on(async { handle.catalog_at(0, epoch).await })
            .map(|catalog| catalog.version())
            .map_err(|refusal| {
                let detail = if refusal.code() == ErrorCode::Protocol {
                    0
                } else {
                    refusal.detail()
                };
                (refusal.code(), detail)
            });
        // L3.20: never a version being checked or withdrawn.
        assert_eq!(got, said, "at epoch {epoch}");
    }

    #[invariant]
    fn agrees(&mut self, _tc: TestCase) {
        let Some(handle) = self.handle() else { return };
        let current = self.model.current();
        let held: Option<Vec<(u64, u64)>> = self.runtime.block_on(async {
            let mut held = Vec::new();
            for version in 1..=current {
                let catalog = handle.catalog(0, version).await.ok()?;
                held.push((catalog.version(), catalog.epoch()));
            }
            Some(held)
        });
        // L3.17: numbered from 1 without gap, each at the epoch the model assigned.
        let expected: Vec<(u64, u64)> = self
            .model
            .versions
            .iter()
            .enumerate()
            .map(|(at, version)| (at as u64 + 1, version.effective.unwrap_or(0))) // lossless, as above
            .collect();
        assert_eq!(held, Some(expected));
        let recorded = self
            .runtime
            .block_on(async { handle.defaults(0).await })
            .map(|defaults| defaults.predicates().to_vec());
        let model: Vec<String> = self
            .model
            .defaults
            .iter()
            .map(|predicate| predicate.to_string())
            .collect();
        assert_eq!(recorded.ok(), Some(model));
    }
}

/// L3.17 to L3.28: any sequence of Declares of both kinds, default entries, closes, reports, restarts and Catalog requests at an
/// epoch numbers the versions without gap, refuses a stale base, answers each Declare as the model of `PROTOCOL.md` §11.13 does,
/// and never answers a version being checked or withdrawn at an epoch.
#[hegel::test(test_cases = 30)]
fn test_the_catalog_agrees_with_its_model(tc: TestCase) {
    let catalog = Catalog::new();
    assert!(catalog.is_some(), "the group started");
    if let Some(catalog) = catalog {
        hegel::stateful::machine(catalog).steps(25).run(tc);
    }
}

/// L3.29: a Declare building a version of the cap, 16,777,216 bytes, and a drawn number of bytes more is refused with code 7,
/// `detail` the cap, the catalog unchanged.
#[hegel::test(test_cases = 3)]
fn test_a_version_past_the_cap_is_refused(tc: TestCase) {
    let past = tc.draw(gs::integers::<usize>().min_value(1).max_value(64));
    let outcome = past_cap(past);
    assert_eq!(outcome, Some((ErrorCode::Refused, 16_777_216, 0)));
}

/// A Declare of one predicate whose IRI takes the version `past` bytes past the cap: its refusal's code and `detail`, and the
/// current version after it.
fn past_cap(past: usize) -> Option<(ErrorCode, u64, u64)> {
    use ultragraphdb_core_catalog::CatalogContents;
    use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
    let length = |iri: usize| -> Option<usize> {
        let node = oxrdf::NamedNode::new(format!("http://ex.org/{}", "a".repeat(iri))).ok()?;
        let contents = CatalogContents::new(
            0,
            1,
            vec![Declaration::Predicate(PredicateDeclaration::new(node))],
        );
        Some(contents.encode().ok()?.len())
    };
    let overhead = length(1_000)? - 1_000;
    let iri = 16_777_216 + past - overhead;
    let runtime = Runtime::new().ok()?;
    let clock = Clock::new();
    let storage = SimStorage::new(3);
    runtime.block_on(async {
        let group = recovered(&storage, &start_of(8, 50)?, &clock).await?;
        let (handle, run) = system_run(group, clock.reading());
        let run = compio::runtime::spawn(run);
        let declared = vec![Declared::Predicate(DeclaredPredicate::new(format!(
            "http://ex.org/{}",
            "a".repeat(iri)
        )))];
        let refusal = handle.declare_version(0, 0, declared).await.err()?;
        let current = handle
            .catalog(0, 0)
            .await
            .map_or(0, |catalog| catalog.version());
        drop(handle);
        run.await.ok()?.ok()?;
        Some((refusal.code(), refusal.detail(), current))
    })
}
