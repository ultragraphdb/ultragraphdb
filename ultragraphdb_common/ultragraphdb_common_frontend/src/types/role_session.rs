/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

use compio::runtime::spawn;
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_common_role::{Answer, RoleClient, RoleCluster, Session};
use ultragraphdb_common_service::{Readiness, RoleKind as ServiceRole};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CatalogAnnouncement, CatalogView, CloseRequestView, Command,
    DefaultEntry, DefaultEntryView, EpochClose, ErrorCode, ErrorReply, FLAG_LAST, FrameView,
    Header, Join, JoinView, Lease, LeaseView, MembersView, PlacementView, ReportKind, ReportView,
    RequestTermView, RoleKind, SubscribeView,
};

use crate::types::owner_handle::OwnerHandle;
use crate::types::report_asked::ReportAsked;
use crate::types::role_links::RoleLinks;

/// How long a failed connection back waits before the next.
const RETRY: std::time::Duration = std::time::Duration::from_millis(100);

/// What a frontend's role connections share on their core: the connections back, and its readiness.
#[derive(Debug, Clone)]
pub struct RoleShared {
    cluster: RoleCluster,
    owner: OwnerHandle,
    links: RoleLinks,
    readiness: Readiness,
}

impl RoleShared {
    /// What the core's role connections share.
    #[must_use]
    pub fn new(
        cluster: RoleCluster,
        owner: OwnerHandle,
        links: RoleLinks,
        readiness: Readiness,
    ) -> Self {
        Self {
            cluster,
            owner,
            links,
            readiness,
        }
    }
}

/// One role connection at the frontend's role address. Each request it answers awaits the owner alone, as
/// [`OwnerHandle`] says, and the connection awaits each answer to its end, so only the runtime's end drops one.
#[derive(Debug)]
pub struct RoleSession {
    shared: RoleShared,
    subscription: Option<(u64, u64)>,
}

impl RoleSession {
    /// A session over what the core shares.
    #[must_use]
    pub fn new(shared: RoleShared) -> Self {
        Self {
            shared,
            subscription: None,
        }
    }

    fn header(&self, stream: u64, command: Command) -> Header {
        Header::new(self.shared.cluster.id(), command)
            .with_stream(stream)
            .with_flags(FLAG_LAST)
    }

    async fn join(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let view = JoinView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let join = joined(&view);
        self.shared.owner.join(join.clone()).await?;
        self.shared.links.joined(&join);
        spawn(connect_back(
            self.shared.clone(),
            join.address().to_string(),
            join.kind(),
        ))
        .detach();
        built(&join, self.header(stream, Command::Join))
    }

    async fn members(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = MembersView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        if !asked.is_empty() {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!("a Members request holds {} members", asked.len()),
            ));
        }
        let members = self.shared.owner.members().await.ok_or_else(stopped)?;
        built(&members, self.header(stream, Command::Members))
    }

    async fn lease(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = LeaseView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let (prefix, partition, size) = (asked.type_prefix(), asked.partition(), asked.size());
        let start = self.shared.owner.lease(prefix, partition, size).await?;
        let reply = Lease::request(prefix, partition, size).with_block(start);
        built(&reply, self.header(stream, Command::Lease))
    }

    async fn placement(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = PlacementView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let reply = self.shared.owner.placement(asked.epoch()).await?;
        built(&reply, self.header(stream, Command::Placement))
    }

    async fn catalog(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = CatalogView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let reply = self
            .shared
            .owner
            .catalog(asked.tenant(), asked.version())
            .await?;
        built(&reply, self.header(stream, Command::Catalog))
    }

    async fn subscribe(
        &mut self,
        stream: u64,
        frame: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = SubscribeView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        if self.subscription.is_some() {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                "this connection holds a subscription already",
            ));
        }
        self.shared.owner.subscribe(asked.from()).await?;
        self.subscription = Some((stream, asked.from()));
        Ok(Answer::Open)
    }

    /// A partition's first write of a predicate no version declares, recorded durably (`PROTOCOL.md` §13.18).
    async fn default_entry(
        &self,
        stream: u64,
        frame: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked =
            DefaultEntryView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        asked
            .check_request()
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let predicate = asked.predicate().to_string();
        let found = self
            .shared
            .owner
            .record_default(asked.tenant(), predicate.clone())
            .await?;
        let reply = DefaultEntry::request(asked.tenant(), predicate).with_found(found);
        built(&reply, self.header(stream, Command::DefaultEntry))
    }

    /// A partition's report on a catalog version, answered with its fields once the group holds it (`PROTOCOL.md` §13.19).
    async fn report(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = ReportView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let body = asked
            .to_body()
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        let report = ReportAsked::new(
            Tenant::new(asked.tenant()),
            asked.version(),
            asked.partition(),
            refusal_of(&asked),
        );
        self.shared.owner.report(report).await?;
        built(&body, self.header(stream, Command::Report))
    }

    async fn close_request(&self, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked =
            CloseRequestView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        self.shared.owner.close_request(asked.epoch()).await?;
        Ok(Answer::Done)
    }
}

impl Session for RoleSession {
    /// A dropped future leaves what [`RoleSession`] says.
    async fn answer(&mut self, stream: u64, received: ReceivedFrame) -> Answer {
        let view = match received.view() {
            Ok(view) => view,
            Err(rule) => return Answer::Refuse(ErrorReply::protocol(&rule)),
        };
        let frame = &view;
        let command = frame.command();
        let answer = if command == Command::Join {
            self.join(stream, frame).await
        } else if command == Command::Members {
            self.members(stream, frame).await
        } else if command == Command::Lease {
            self.lease(stream, frame).await
        } else if command == Command::Placement {
            self.placement(stream, frame).await
        } else if command == Command::Catalog {
            self.catalog(stream, frame).await
        } else if command == Command::Subscribe {
            self.subscribe(stream, frame).await
        } else if command == Command::CloseRequest {
            self.close_request(frame).await
        } else if command == Command::DefaultEntry {
            self.default_entry(stream, frame).await
        } else if command == Command::Report {
            self.report(stream, frame).await
        } else {
            Err(ErrorReply::new(
                ErrorCode::Unavailable,
                format!("command {} is not hosted here yet", command.number()),
            ))
        };
        answer.unwrap_or_else(Answer::Refuse)
    }

    /// The next close of this connection's subscription. Its cursor moves only once a close came back, and a wait dropped
    /// for a request that arrived first asks again, so a close is neither lost nor sent twice.
    async fn next_push(&mut self) -> (u64, Vec<u8>) {
        loop {
            let Some((stream, after)) = self.subscription else {
                return std::future::pending().await;
            };
            let close = match self.shared.owner.next_close(after).await {
                Ok(close) => close,
                Err(refusal) => {
                    // A subscriber too far behind for the closes held has nothing left to read here, and is told so on its
                    // stream, which this ends (`PROTOCOL.md` §13.3).
                    tracing::warn!(after, reason = refusal.message(), "a subscription ended");
                    self.subscription = None;
                    let header = Header::new(self.shared.cluster.id(), Command::Error)
                        .with_stream(stream)
                        .with_flags(FLAG_LAST);
                    match refusal.frame(header) {
                        Ok(frame) => return (stream, frame),
                        Err(error) => {
                            tracing::error!(%error, "a subscription's end could not be framed");
                            continue;
                        }
                    }
                }
            };
            let cluster = self.shared.cluster.id();
            match close_frame(cluster, stream, &close) {
                Ok(frame) => {
                    self.subscription = Some((stream, close.epoch().get()));
                    return (stream, frame);
                }
                Err(error) => {
                    tracing::error!(%error, "a close could not be framed");
                    self.subscription = None;
                }
            }
        }
    }

    fn cancelled(&mut self, stream: u64) {
        if self.subscription.is_some_and(|(held, _)| held == stream) {
            self.subscription = None;
        }
    }
}

/// What `report` says the data refused: its predicate and the term at fault as text; a failed build names its predicate.
fn refusal_of(report: &ReportView<'_>) -> Option<(String, String)> {
    let predicate = report.predicate().unwrap_or_default().to_string();
    match report.kind() {
        ReportKind::Passed => None,
        ReportKind::BuildFailed => Some((predicate, "its index build".to_string())),
        ReportKind::Refused => {
            let at_fault = report.at_fault().map(|term| match term {
                RequestTermView::Iri(iri) => iri.to_string(),
                RequestTermView::BlankNode(label) => format!("_:{label}"),
                RequestTermView::Literal { lexical, .. }
                | RequestTermView::LanguageLiteral { lexical, .. } => format!("\"{lexical}\""),
                RequestTermView::Triple { .. } => "a triple term".to_string(),
            });
            Some((predicate, at_fault.unwrap_or_default()))
        }
    }
}

/// `close` as the one EpochClose frame it takes: its lists hold at most 4,352 records of 16 bytes, well inside the
/// smallest `max_frame` a peer may announce, so `more` is never set.
fn close_frame(
    cluster: u64,
    stream: u64,
    close: &Close,
) -> Result<Vec<u8>, ultragraphdb_error_protocol::ProtocolError> {
    let mut body =
        EpochClose::new(close.epoch().get(), close.time_us()).with_placement(close.placement());
    let mut announcements: Vec<(u64, u64)> = close
        .catalog()
        .iter()
        .map(|version| (version.tenant().get(), version.version()))
        .collect();
    announcements.sort_unstable();
    for (tenant, version) in announcements {
        body.push_announcement(CatalogAnnouncement::new(tenant, version));
    }
    let mut evictions = close.evictions().to_vec();
    evictions.sort_unstable();
    for session in evictions {
        body.push_eviction(session.to_be_bytes());
    }
    body.frame(Header::new(cluster, Command::EpochClose).with_stream(stream))
}

/// The Join's own fields, held past its frame.
fn joined(join: &JoinView<'_>) -> Join {
    if join.kind() == RoleKind::Compute {
        return Join::compute(join.address());
    }
    let partitions = (0..join.partition_count())
        .filter_map(|index| join.partition(index))
        .collect::<Vec<u32>>();
    Join::storage(join.address(), partitions)
}

fn built<B: BodyBuilder>(body: &B, header: Header) -> Result<Answer, ErrorReply> {
    body.frame(header)
        .map(Answer::Reply)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

fn stopped() -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, "the frontend's owner stopped")
}

#[expect(
    clippy::disallowed_methods,
    reason = "a deadline in the shell, which no simulation replays"
)]
fn now() -> Instant {
    Instant::now()
}

/// Connects back to the role that joined at `text`, retrying until the deadline; the first partition reached makes the
/// frontend ready. A connection a later Join of the address brings replaces this one.
/// Spawned detached: a future dropped at the runtime's end leaves no connection back.
async fn connect_back(shared: RoleShared, text: String, kind: RoleKind) {
    let Ok(address) = Address::parse(&text) else {
        tracing::warn!(address = %text, "a role joined at an address that does not parse");
        return;
    };
    let due = now() + shared.cluster.deadline();
    let name = ServiceRole::Frontend.to_string();
    loop {
        let secret = shared.cluster.secret().password();
        match RoleClient::dial(&address, shared.cluster.id(), &name, secret).await {
            Ok(client) => {
                shared.links.connected(text, client);
                if kind == RoleKind::Storage {
                    shared.readiness.set(true);
                }
                return;
            }
            Err(error) if now() >= due => {
                tracing::warn!(address = %text, %error, "the frontend could not connect back");
                return;
            }
            Err(_) => compio::time::sleep(RETRY).await,
        }
    }
}
