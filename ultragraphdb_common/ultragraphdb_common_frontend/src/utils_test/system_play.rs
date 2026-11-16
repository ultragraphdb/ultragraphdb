/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use ultragraphdb_cluster_placement::{NodeId, PlacementMap, TenantRun as PlacedRun};
use ultragraphdb_common_role::Answer;
use ultragraphdb_core_ids::{Epoch, Tenant};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, BucketRange, Catalog, CatalogAnnouncement, CatalogView, Challenge,
    ChallengeView, Check, CheckOutcome, CheckView, Command, Declare, DeclareView, Defaults,
    DefaultsView, EpochClose, ErrorCode, ErrorReply, FLAG_LAST, FrameView, Header, MAX_FRAME_FLOOR,
    MAX_FRAME_SIZE, Members, Placement, PreferredLeader, Session, SessionView, TenantRun, Verify,
    VerifyView,
};

use crate::utils_test::double_frame::DoubleFrame;

/// What the played group holds.
#[derive(Debug, Default)]
struct Played {
    closed: u64,
    subscriptions: Vec<DoubleFrame>,
    partitions: u32,
    tenants: Vec<u64>,
    storage: Vec<(u32, String)>,
    compute: Vec<String>,
    /// Each session's outcomes, answered in order, the last for every check after it.
    outcomes: BTreeMap<[u8; 16], VecDeque<CheckOutcome>>,
    holding: bool,
    held: Vec<DoubleFrame>,
    versions: BTreeMap<u64, Vec<(u64, u64, Vec<u8>)>>,
    defaults: BTreeMap<u64, Vec<String>>,
    principals: BTreeMap<String, (u64, ScramVerifier)>,
    refusals: BTreeMap<u16, ErrorReply>,
    silent: Vec<u16>,
}

/// The system group a test plays at a [`crate::RoleDouble`]'s address (task 5.2): [`SystemPlay::answer`] is the double's
/// script. It answers each command an endpoint core asks as `PROTOCOL.md` §11.12's table has the group serve the frontend,
/// from what the test set: Subscribe with the latest close and every close [`SystemPlay::close`] pushes, Placement of an even
/// map over its partitions and its tenants' runs, Members, Session, each Check passed or as [`SystemPlay::outcome`] says,
/// Ended with nothing, Challenge and Verify against the principals it holds, Declare building the next version on the
/// current one, Defaults and Catalog across frames. A clone shares what it holds, on the test's one thread.
#[derive(Debug, Clone)]
pub struct SystemPlay {
    cluster: u64,
    played: Rc<RefCell<Played>>,
}

impl SystemPlay {
    /// The group of `cluster` placing `partitions` partitions, holding tenant 0, at the close of epoch 1.
    #[must_use]
    pub fn new(cluster: u64, partitions: u32) -> Self {
        let played = Played {
            closed: 1,
            partitions,
            tenants: vec![0],
            ..Played::default()
        };
        Self {
            cluster,
            played: Rc::new(RefCell::new(played)),
        }
    }

    /// The group holding `tenants` beside tenant 0.
    #[must_use]
    pub fn with_tenants(self, tenants: &[u64]) -> Self {
        self.played.borrow_mut().tenants.extend_from_slice(tenants);
        self
    }

    /// The group holding the principal `principal` named `name`, of `verifier`.
    #[must_use]
    pub fn with_principal(self, name: &str, principal: u64, verifier: ScramVerifier) -> Self {
        let _replaced = self
            .played
            .borrow_mut()
            .principals
            .insert(name.to_string(), (principal, verifier));
        self
    }

    /// A storage process holding `partition` listed by Members at `address`.
    pub fn storage(&self, partition: u32, address: &str) {
        self.played
            .borrow_mut()
            .storage
            .push((partition, address.to_string()));
    }

    /// A compute process listed by Members at `address`.
    pub fn compute(&self, address: &str) {
        self.played.borrow_mut().compute.push(address.to_string());
    }

    /// The next check of `session` answers `outcome`, after those queued before it; the last queued answers every check
    /// after it.
    pub fn outcome(&self, session: [u8; 16], outcome: CheckOutcome) {
        self.played
            .borrow_mut()
            .outcomes
            .entry(session)
            .or_default()
            .push_back(outcome);
    }

    /// Holds every Check frame unanswered until [`SystemPlay::release`].
    pub fn hold_checks(&self) {
        self.played.borrow_mut().holding = true;
    }

    /// Answers the Check frames held, and holds no more.
    pub fn release(&self) {
        let held = {
            let mut played = self.played.borrow_mut();
            played.holding = false;
            std::mem::take(&mut played.held)
        };
        for frame in held {
            if let Answer::Reply(reply) = self.check(&frame) {
                frame.answer_with(reply);
            }
        }
    }

    /// Answers each request of the command numbered `command` with `refusal`.
    pub fn refuse(&self, command: Command, refusal: ErrorReply) {
        let _replaced = self
            .played
            .borrow_mut()
            .refusals
            .insert(command.number(), refusal);
    }

    /// Answers no request of `command`: each stays open.
    pub fn silence(&self, command: Command) {
        self.played.borrow_mut().silent.push(command.number());
    }

    /// `tenant`'s version `version` taking effect at `epoch` (0 while pending), its bytes `bytes`.
    pub fn version(&self, tenant: u64, version: u64, epoch: u64, bytes: Vec<u8>) {
        self.played
            .borrow_mut()
            .versions
            .entry(tenant)
            .or_default()
            .push((version, epoch, bytes));
    }

    /// `tenant`'s default-entry records, in order.
    pub fn defaults(&self, tenant: u64, predicates: Vec<String>) {
        let _replaced = self.played.borrow_mut().defaults.insert(tenant, predicates);
    }

    /// The latest close.
    #[must_use]
    pub fn closed(&self) -> u64 {
        self.played.borrow().closed
    }

    /// Closes `epoch`, assigning the placement version `placement` when not 0 and announcing each `(tenant, version)`, on
    /// every subscription.
    pub fn close(&self, epoch: u64, placement: u64, announced: &[(u64, u64)]) {
        let subscriptions = {
            let mut played = self.played.borrow_mut();
            played.closed = played.closed.max(epoch);
            played.subscriptions.clone()
        };
        let mut close = EpochClose::new(epoch, 0);
        if placement != 0 {
            close = close.with_placement(placement);
        }
        for (tenant, version) in announced {
            close.push_announcement(CatalogAnnouncement::new(*tenant, *version));
        }
        for subscription in subscriptions {
            let header =
                Header::new(self.cluster, Command::EpochClose).with_stream(subscription.stream());
            if let Ok(frame) = close.frame(header) {
                subscription.answer_with(frame);
            }
        }
    }

    /// Ends every subscription with Error 1, as a group no longer holding its next close does.
    pub fn end_subscriptions(&self) {
        let subscriptions = std::mem::take(&mut self.played.borrow_mut().subscriptions);
        let refusal = ErrorReply::new(ErrorCode::Protocol, "the next close is no longer held");
        for subscription in subscriptions {
            if let Ok(frame) = self.last(subscription.stream(), &refusal) {
                subscription.answer_with(frame);
            }
        }
    }

    /// The double's script: the answer to `frame`.
    #[must_use]
    pub fn answer(&self, frame: &DoubleFrame) -> Answer {
        let command = frame.command();
        if self.played.borrow().silent.contains(&command.number()) {
            return Answer::Open;
        }
        if let Some(refusal) = self.played.borrow().refusals.get(&command.number()) {
            return Answer::Refuse(refusal.clone());
        }
        let stream = frame.stream();
        if command == Command::Subscribe {
            self.subscribe(frame)
        } else if command == Command::Placement {
            self.reply(stream, &self.placement())
        } else if command == Command::Members {
            self.reply(stream, &self.members())
        } else if command == Command::Session {
            self.session(frame)
        } else if command == Command::Check {
            self.check(frame)
        } else if command == Command::Ended || command == Command::CloseRequest {
            Answer::Done
        } else if command == Command::Challenge {
            self.challenge(frame)
        } else if command == Command::Verify {
            self.verify(frame)
        } else if command == Command::Declare {
            self.declare(frame)
        } else if command == Command::Defaults {
            self.defaults_of(frame)
        } else if command == Command::Catalog {
            self.catalog(frame)
        } else {
            Answer::Refuse(ErrorReply::new(
                ErrorCode::Protocol,
                format!("{command:?} is not served here"),
            ))
        }
    }

    fn view<'a>(&self, frame: &'a DoubleFrame) -> Option<FrameView<'a>> {
        FrameView::validate(frame.bytes(), self.cluster, MAX_FRAME_SIZE).ok()
    }

    fn last<B: BodyBuilder>(&self, stream: u64, body: &B) -> Result<Vec<u8>, ErrorReply> {
        let header = Header::new(self.cluster, B::COMMAND)
            .with_stream(stream)
            .with_flags(FLAG_LAST);
        body.frame(header)
            .map_err(|rule| ErrorReply::protocol(&rule))
    }

    fn reply<B: BodyBuilder>(&self, stream: u64, body: &B) -> Answer {
        match self.last(stream, body) {
            Ok(frame) => Answer::Reply(frame),
            Err(refusal) => Answer::Refuse(refusal),
        }
    }

    /// Answers `frames` in order on `frame`'s stream.
    fn frames(frame: &DoubleFrame, frames: Vec<Vec<u8>>) -> Answer {
        for each in frames {
            frame.answer_with(each);
        }
        Answer::Open
    }

    fn subscribe(&self, frame: &DoubleFrame) -> Answer {
        let closed = {
            let mut played = self.played.borrow_mut();
            played.subscriptions.push(frame.clone());
            played.closed
        };
        let header = Header::new(self.cluster, Command::EpochClose).with_stream(frame.stream());
        if let Ok(close) = EpochClose::new(closed, 0).frame(header) {
            frame.answer_with(close);
        }
        Answer::Open
    }

    fn placement(&self) -> Placement {
        let played = self.played.borrow();
        let mut reply = Placement::reply(1, 0, 1, 0);
        if let Ok(map) = PlacementMap::even(played.partitions.max(1), NodeId::new(1)) {
            for (range, partition) in map.ranges_at(Epoch::ZERO) {
                reply.push_range(BucketRange::new(
                    range.start(),
                    range.end(),
                    partition.get(),
                ));
            }
            for partition in 1..=played.partitions.max(1) {
                reply.push_leader(PreferredLeader::new(partition, 1));
            }
        }
        for tenant in &played.tenants {
            reply.push_run(TenantRun::new(
                *tenant,
                PlacedRun::of(Tenant::new(*tenant)).get(),
            ));
        }
        reply
    }

    fn members(&self) -> Members {
        let played = self.played.borrow();
        let mut members = Members::new();
        // Compute first, by address, then storage, by partition (`PROTOCOL.md` §13.2).
        let mut compute = played.compute.clone();
        compute.sort();
        for address in compute {
            members.push_compute(address);
        }
        let mut storage = played.storage.clone();
        storage.sort();
        for (partition, address) in storage {
            members.push_storage(partition, address);
        }
        members
    }

    fn session(&self, frame: &DoubleFrame) -> Answer {
        let Some(asked) = self.view(frame) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        match SessionView::from_frame(&asked) {
            Ok(session) => self.reply(
                frame.stream(),
                &Session::new(session.session(), session.principal()),
            ),
            Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
        }
    }

    fn check(&self, frame: &DoubleFrame) -> Answer {
        if self.played.borrow().holding {
            self.played.borrow_mut().held.push(frame.clone());
            return Answer::Open;
        }
        let Some(asked) = self.view(frame) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        let checks = match CheckView::from_frame(&asked) {
            Ok(checks) => checks,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let mut played = self.played.borrow_mut();
        let answered = (0..checks.len())
            .filter_map(|at| checks.get(at))
            .map(|check| {
                let outcome = match played.outcomes.get_mut(&check.session()) {
                    Some(queued) if queued.len() > 1 => queued.pop_front(),
                    Some(queued) => queued.front().copied(),
                    None => None,
                };
                check.with_outcome(outcome.unwrap_or(CheckOutcome::Passed))
            })
            .collect();
        drop(played);
        self.reply(frame.stream(), &Check::new(answered))
    }

    fn verifier_of(&self, name: &str) -> (u64, ScramVerifier) {
        self.played
            .borrow()
            .principals
            .get(name)
            .cloned()
            .unwrap_or_else(|| (0, ScramVerifier::unknown(&[3; 32], name, 4_096)))
    }

    fn challenge(&self, frame: &DoubleFrame) -> Answer {
        let Some(asked) = self.view(frame) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        match ChallengeView::from_frame(&asked) {
            Ok(challenge) => {
                let (_, verifier) = self.verifier_of(challenge.name());
                let reply = Challenge::request(challenge.name())
                    .with_salt(verifier.iterations(), verifier.salt().to_vec());
                self.reply(frame.stream(), &reply)
            }
            Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
        }
    }

    fn verify(&self, frame: &DoubleFrame) -> Answer {
        let Some(asked) = self.view(frame) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        match VerifyView::from_frame(&asked) {
            Ok(verify) => {
                let (principal, verifier) = self.verifier_of(verify.name());
                let request =
                    Verify::request(verify.name(), verify.auth_message(), *verify.proof());
                let reply = match verifier.sign(verify.auth_message(), verify.proof()) {
                    Some(signature) if principal != 0 => {
                        request.with_verified(principal, signature)
                    }
                    Some(_) | None => request,
                };
                self.reply(frame.stream(), &reply)
            }
            Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
        }
    }

    fn current(&self, tenant: u64) -> u64 {
        self.played
            .borrow()
            .versions
            .get(&tenant)
            .and_then(|versions| versions.iter().map(|(version, ..)| *version).max())
            .unwrap_or(0)
    }

    fn declare(&self, frame: &DoubleFrame) -> Answer {
        let Some(asked) = self.view(frame) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        let declare = match DeclareView::from_frame(&asked) {
            Ok(declare) => declare,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let (tenant, current) = (declare.tenant(), self.current(declare.tenant()));
        let request = match declare.drop_kind() {
            Some(kind) => Declare::drop(
                tenant,
                declare.principal(),
                kind,
                declare.target().map(str::to_string),
            ),
            None => {
                if declare.built_on() != current {
                    return Answer::Refuse(ErrorReply::new(
                        ErrorCode::Conflict,
                        format!(
                            "catalog version {} is no longer current, {current} is",
                            declare.built_on()
                        ),
                    ));
                }
                let declared = (0..declare.len())
                    .filter_map(|at| declare.declaration(at))
                    .collect();
                Declare::declarations(tenant, current, declare.principal(), declared)
            }
        };
        let version = current + 1;
        self.version(tenant, version, 0, Vec::new());
        // A Drop's reply names no commit version (`PROTOCOL.md` §13.27).
        let commit_version = if declare.drop_kind().is_some() {
            0
        } else {
            (self.closed() + 1) << 22
        };
        self.reply(
            frame.stream(),
            &request.with_version(version, commit_version),
        )
    }

    fn defaults_of(&self, frame: &DoubleFrame) -> Answer {
        let Some(asked) = self.view(frame) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        let tenant = match DefaultsView::from_frame(&asked) {
            Ok(defaults) => defaults.tenant(),
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let predicates = self
            .played
            .borrow()
            .defaults
            .get(&tenant)
            .cloned()
            .unwrap_or_default();
        let header = Header::new(self.cluster, Command::Defaults).with_stream(frame.stream());
        match Defaults::reply(tenant, 1, predicates).frames(header, MAX_FRAME_FLOOR) {
            Ok(frames) => Self::frames(frame, frames),
            Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
        }
    }

    fn catalog(&self, frame: &DoubleFrame) -> Answer {
        let Some(asked) = self.view(frame) else {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Protocol, "a frame refused"));
        };
        let catalog = match CatalogView::from_frame(&asked) {
            Ok(catalog) => catalog,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let (tenant, version, epoch) = (catalog.tenant(), catalog.version(), catalog.epoch());
        let played = self.played.borrow();
        let versions = played.versions.get(&tenant).cloned().unwrap_or_default();
        drop(played);
        let found = versions
            .iter()
            .filter(|(held, effective, _)| match (version, epoch) {
                (0, 0) => true,
                (0, at) => *effective != 0 && *effective <= at,
                (named, _) => *held == named,
            })
            .max_by_key(|(held, ..)| *held)
            .cloned();
        let Some((held, effective, bytes)) = found else {
            return Answer::Refuse(
                ErrorReply::new(
                    ErrorCode::Refused,
                    format!("tenant {tenant} holds no version {version}"),
                )
                .with_detail(version),
            );
        };
        let header = Header::new(self.cluster, Command::Catalog).with_stream(frame.stream());
        match Catalog::reply(tenant, held, effective, 1, bytes).frames(header, MAX_FRAME_FLOOR) {
            Ok(frames) => Self::frames(frame, frames),
            Err(rule) => Answer::Refuse(ErrorReply::protocol(&rule)),
        }
    }
}
