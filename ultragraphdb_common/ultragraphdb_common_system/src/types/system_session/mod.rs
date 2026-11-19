/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod system_session_drop;
mod system_session_frontend;
mod system_session_session;

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_common_role::{Answer, Push, RoleCluster};
use ultragraphdb_core_ids::Tenant;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, CatalogAnnouncement, CatalogView, CloseRequestView, Command,
    DefaultEntry, DefaultEntryView, EpochClose, ErrorCode, ErrorReply, FLAG_LAST, FrameView,
    Header, Join, JoinView, Lease, LeaseView, MAX_FRAME_FLOOR, MembersView, PlacementView,
    ReportKind, ReportView, RequestTermView, RoleKind, SubscribeView,
};

use crate::types::report_asked::ReportAsked;
use crate::types::system_handle::SystemHandle;

/// What the system group's role connections share on their core: the cluster and the way to the owner.
#[derive(Debug, Clone)]
pub struct SystemShared {
    cluster: RoleCluster,
    owner: SystemHandle,
}

impl SystemShared {
    /// What the core's role connections share.
    #[must_use]
    pub fn new(cluster: RoleCluster, owner: SystemHandle) -> Self {
        Self { cluster, owner }
    }
}

/// One role connection to the system group. It serves the commands `PROTOCOL.md` §11.12's endpoint table lists for the
/// system group's role address, each only from the role the table has send it, the role being the connection's SCRAM
/// username; it refuses every other as `CommandNotServed`, closing the connection.
///
/// Each request it answers in its turn awaits the owner alone, as [`SystemHandle`] says, and the connection awaits each
/// such answer to its end. A request whose answer waits for more than the group's next sync, a Session's registration and
/// a Declare's version, answers [`Answer::Open`] and is pushed once the group answers, so the Checks behind it on the
/// connection are answered meanwhile; an answer of more than one frame is pushed frame by frame after its first. Only the
/// runtime's end drops a session; when it is dropped, the group holds the marks its checks began for twice
/// `transaction_ms` (`PROTOCOL.md` §11.5).
#[derive(Debug)]
pub struct SystemSession {
    shared: SystemShared,
    subscription: Option<(u64, u64)>,
    /// The name the connection's exchange named, read at its first message.
    named: Option<String>,
    /// The connection's role, once it authenticated.
    role: Option<RoleKind>,
    /// The group's number for this connection, asked for at its first Check or Ended.
    connection: Option<u64>,
    /// The largest frame the peer takes, from its Hello.
    max_frame: usize,
    /// Where an answer pushed later goes, and where [`ultragraphdb_common_role::Session::next_push`] takes it from.
    /// Unbounded: each holds the frames of one request the connection opened, so the connection's open streams bound it.
    pushes: UnboundedSender<(u64, Push)>,
    pushed: UnboundedReceiver<(u64, Push)>,
}

impl SystemSession {
    /// A session over what the core shares.
    #[must_use]
    pub fn new(shared: SystemShared) -> Self {
        let (pushes, pushed) = unbounded();
        Self {
            shared,
            subscription: None,
            named: None,
            role: None,
            connection: None,
            max_frame: MAX_FRAME_FLOOR,
            pushes,
            pushed,
        }
    }

    fn header(&self, stream: u64, command: Command) -> Header {
        header_of(self.shared.cluster.id(), stream, command)
    }

    async fn join(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let view = JoinView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let join = joined(&view);
        self.shared.owner.join(join.clone()).await?;
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

    /// A Catalog request: the version it names, the current one for `version` 0, or for `version` 0 naming an `epoch` the
    /// version a client may read there; answered across frames within the requester's `max_frame` (`PROTOCOL.md` §13.7).
    async fn catalog(&self, stream: u64, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked = CatalogView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let owner = &self.shared.owner;
        let reply = if asked.version() == 0 && asked.epoch() != 0 {
            owner.catalog_at(asked.tenant(), asked.epoch()).await?
        } else {
            owner.catalog(asked.tenant(), asked.version()).await?
        };
        let frames = reply
            .frames(self.header(stream, Command::Catalog), self.max_frame)
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        Ok(self.answer_frames(stream, frames))
    }

    /// A Subscribe: from the close it names, or for `latest` 1, which a compute node's connection alone may ask, from the
    /// latest durable close (`PROTOCOL.md` §13.3).
    async fn subscribe(
        &mut self,
        stream: u64,
        frame: &FrameView<'_>,
    ) -> Result<Answer, ErrorReply> {
        let asked = SubscribeView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let role = self.role.ok_or_else(unauthenticated)?;
        asked
            .check_sender(role)
            .map_err(|rule| ErrorReply::protocol(&rule))?;
        if self.subscription.is_some() {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                "this connection holds a subscription already",
            ));
        }
        let after = if asked.latest() {
            // The close after the cursor is the latest durable one, or the close of epoch 1 when none exists yet.
            self.shared.owner.latest_close().await?.saturating_sub(1)
        } else {
            self.shared.owner.subscribe(asked.from()).await?;
            asked.from()
        };
        self.subscription = Some((stream, after));
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

    /// A CloseRequest, answered by no frame: of `partition` 0 from compute alone, and of `partition` 1 or above from
    /// storage alone (`PROTOCOL.md` §13.16); every refusal is dropped unanswered.
    async fn close_request(&self, frame: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let asked =
            CloseRequestView::from_frame(frame).map_err(|rule| ErrorReply::protocol(&rule))?;
        let role = self.role.ok_or_else(unauthenticated)?;
        let partition = asked.partition();
        let named = match role {
            RoleKind::Compute => partition == 0,
            RoleKind::Storage => partition != 0,
            // No connection acts as the retired frontend: `role_named` names no such role.
            RoleKind::Frontend => false,
        };
        if !named {
            let rule = ProtocolError::field_value(
                Command::CloseRequest.number(),
                "partition",
                u64::from(partition),
            );
            return Err(ErrorReply::protocol(&rule));
        }
        self.shared.owner.close_request(asked.epoch()).await?;
        Ok(Answer::Done)
    }

    /// `frames`, an answer on `stream`: its first as the reply, the rest pushed after it in order.
    fn answer_frames(&self, stream: u64, frames: Vec<Vec<u8>>) -> Answer {
        let mut frames = frames.into_iter();
        let Some(first) = frames.next() else {
            return Answer::Done;
        };
        for frame in frames {
            // A session holds the receiver as long as the sender, so the send cannot fail.
            let _queued = self.pushes.unbounded_send((stream, Push::Frame(frame)));
        }
        Answer::Reply(first)
    }

    /// The group's number for this connection, asked for once.
    async fn connection(&mut self) -> Result<u64, ErrorReply> {
        if let Some(connection) = self.connection {
            return Ok(connection);
        }
        let connection = self.shared.owner.connect().await.ok_or_else(stopped)?;
        self.connection = Some(connection);
        Ok(connection)
    }
}

/// Whether the system group's endpoint table has `role` send `command` (`PROTOCOL.md` §11.12a): a connection not yet
/// authenticated sends nothing its machine hands on.
fn served(role: Option<RoleKind>, command: Command) -> bool {
    let Some(role) = role else {
        return false;
    };
    let (compute, storage) = (role == RoleKind::Compute, role == RoleKind::Storage);
    if BOTH_ROLES.contains(&command) {
        compute || storage
    } else if STORAGE_ALONE.contains(&command) {
        storage
    } else {
        compute && COMPUTE_ALONE.contains(&command)
    }
}

/// The commands both roles send the system group.
const BOTH_ROLES: [Command; 6] = [
    Command::Join,
    Command::Members,
    Command::Placement,
    Command::Catalog,
    Command::Subscribe,
    Command::CloseRequest,
];

/// The commands storage alone sends the system group.
const STORAGE_ALONE: [Command; 3] = [Command::Lease, Command::DefaultEntry, Command::Report];

/// The commands compute alone sends the system group: a client's authentication, its sessions and its declarations.
const COMPUTE_ALONE: [Command; 7] = [
    Command::Challenge,
    Command::Verify,
    Command::Session,
    Command::Check,
    Command::Ended,
    Command::Declare,
    Command::Defaults,
];

/// The role a connection authenticated as `name` acts in: a role connection's SCRAM username is its role's name, `compute`
/// or `storage` (`PROTOCOL.md` §11.12a).
fn role_named(name: &str) -> Option<RoleKind> {
    match name {
        "compute" => Some(RoleKind::Compute),
        "storage" => Some(RoleKind::Storage),
        _ => None,
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

/// The header of an answer on `stream` of `cluster`, flagged as its stream's last.
fn header_of(cluster: u64, stream: u64, command: Command) -> Header {
    Header::new(cluster, command)
        .with_stream(stream)
        .with_flags(FLAG_LAST)
}

fn built<B: BodyBuilder>(body: &B, header: Header) -> Result<Answer, ErrorReply> {
    body.frame(header)
        .map(Answer::Reply)
        .map_err(|rule| ErrorReply::protocol(&rule))
}

fn stopped() -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, "the frontend's owner stopped")
}

/// A request before the connection authenticated, which its machine never hands on.
fn unauthenticated() -> ErrorReply {
    ErrorReply::new(
        ErrorCode::Unauthenticated,
        "the connection has not authenticated",
    )
}
