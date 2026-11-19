/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A client's reads through the compute process's client endpoint, at the level of its frames: a scene's read is a client's
//! Begin and Read (`PROTOCOL.md` §6.2, §11.3), its answer the Result frames or the Error compute sends the client. The
//! administrator logs in over SCRAM, which the played system group answers, and each read's transaction is a tenant of the
//! scene's own.

use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Carrier;
use ultragraphdb_protocol_wire::{
    Begin, BodyBuilder, Command, Credit, Header, MAX_FRAME_SIZE_U32, QuadPattern, Read, ReadLevel,
    TermTable,
};
use ultragraphdb_tests::CLUSTER;
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::cluster::ClusterInfo;
use crate::endpoint_steps::Authenticated;
use crate::scene::{
    ANSWER_WAIT, Batch, DEADLINE_MS, Pattern, ReadAnswer, Refused, Scene, batch_of, refusal_of,
    within,
};

/// A read a client sends: the Begin of its transaction, and the Read in it, over a connection whose Hello names `max_frame`.
#[derive(Debug, Clone)]
pub struct ClientRead {
    pub begin: Begin,
    /// The most rows it asks for, 0 for no limit.
    pub limit: u32,
    pub terms: TermTable,
    pub pattern: QuadPattern,
    /// The key a previous answer of it ended with, empty to start.
    pub resume: Vec<u8>,
    pub max_frame: u32,
}

impl ClientRead {
    /// The Read in the transaction whose Begin went on `transaction`.
    pub fn read_in(&self, transaction: u64) -> Read {
        Read::new(
            transaction,
            self.limit,
            self.terms.clone(),
            self.pattern,
            self.resume.clone(),
        )
    }
}

/// `pattern` read in `scene`'s tenant at `level`, in a transaction read-only or not, with the scene's deadline.
pub fn read_at(
    scene: &Scene,
    pattern: &Pattern,
    read_only: bool,
    level: ReadLevel,
) -> TestResult<ClientRead> {
    let (terms, pattern) = pattern.table()?;
    let begin = Begin::new(scene.tenant, 0, read_only)
        .with_level(level)
        .with_deadline_ms(DEADLINE_MS);
    Ok(ClientRead {
        begin,
        limit: 0,
        terms,
        pattern,
        resume: Vec::new(),
        max_frame: MAX_FRAME_SIZE_U32,
    })
}

/// `pattern` read at snapshot level, read-only, at the version bounding `epoch`.
pub fn snapshot_at(scene: &Scene, pattern: &Pattern, epoch: u64) -> TestResult<ClientRead> {
    let mut read = read_at(scene, pattern, true, ReadLevel::Snapshot)?;
    let bound = Version::bound_of(Epoch::new(epoch)?);
    read.begin = Begin::new(scene.tenant, bound, true)
        .with_level(ReadLevel::Snapshot)
        .with_deadline_ms(DEADLINE_MS);
    Ok(read)
}

/// `pattern` read at snapshot level past everything committed so far.
pub fn snapshot(scene: &Scene, pattern: &Pattern) -> TestResult<ClientRead> {
    snapshot_at(scene, pattern, scene.snapshot_epoch())
}

/// `read`'s answer, on a connection of its own.
pub async fn answer(scene: &Scene, read: &ClientRead) -> TestResult<ReadAnswer> {
    let mut client = EndpointClient::connect(scene.info, read.max_frame).await?;
    answer_on(&mut client, read).await
}

/// The administrator's connection to the compute process's client endpoint, numbering the streams it opens.
pub struct EndpointClient {
    connection: Authenticated<Carrier>,
    next: u64,
}

impl EndpointClient {
    /// A connection to `info`'s compute, its Hello naming `max_frame`, logged in as the administrator.
    pub async fn connect(info: &ClusterInfo, max_frame: u32) -> TestResult<Self> {
        let carrier = Carrier::connect(&Address::parse(&info.compute_address())?).await?;
        let connection = within(
            ANSWER_WAIT,
            "the client's handshake and login",
            Authenticated::framed(carrier, max_frame),
        )
        .await?;
        Ok(Self {
            connection,
            next: 1,
        })
    }

    /// A stream the client has not opened: a client's streams are odd.
    pub fn stream(&mut self) -> u64 {
        let stream = self.next;
        self.next += 2;
        stream
    }

    /// Sends `body` on `stream`.
    pub async fn send_on<B: BodyBuilder>(&mut self, stream: u64, body: &B) -> TestResult {
        let frame = body.frame(Header::new(CLUSTER, B::COMMAND).with_stream(stream))?;
        self.connection.send(frame).await
    }

    /// Sends `body` on a stream of its own: that stream.
    pub async fn send_only<B: BodyBuilder>(&mut self, body: &B) -> TestResult<u64> {
        let stream = self.stream();
        self.send_on(stream, body).await?;
        Ok(stream)
    }

    /// The next frame, failing past [`ANSWER_WAIT`] naming `what` it waited for.
    pub async fn next_frame(&mut self, what: &str) -> TestResult<ReceivedFrame> {
        within(ANSWER_WAIT, what, self.connection.next()).await
    }

    /// Opens `begin`'s transaction: its stream, or compute's refusal of it.
    pub async fn begin(&mut self, begin: &Begin) -> TestResult<Result<u64, Refused>> {
        let stream = self.send_only(begin).await?;
        let answer = self.next_frame("the answer to a Begin").await?;
        if let Some(refused) = refusal_of(&answer)? {
            return Ok(Err(refused));
        }
        let command = answer.view()?.command();
        if command == Command::Begin {
            Ok(Ok(stream))
        } else {
            Err(TestError::new(format!("the Begin answered {command:?}")))
        }
    }

    /// Sends `read`'s Read in the transaction whose Begin went on `transaction`: the read's stream.
    pub async fn send_read(&mut self, transaction: u64, read: &ClientRead) -> TestResult<u64> {
        self.send_only(&read.read_in(transaction)).await
    }
}

/// Opens `read`'s transaction on `client`, sends the read and reads its answer, granting a frame's credit per batch.
pub async fn answer_on(client: &mut EndpointClient, read: &ClientRead) -> TestResult<ReadAnswer> {
    let mut answer = ReadAnswer::default();
    let transaction = match client.begin(&read.begin).await? {
        Ok(transaction) => transaction,
        Err(refused) => {
            answer.refused = Some(refused);
            return Ok(answer);
        }
    };
    let stream = client.send_read(transaction, read).await?;
    loop {
        let batch = match next_of(client).await? {
            Ok(batch) => batch,
            Err(refused) => {
                answer.refused = Some(refused);
                return Ok(answer);
            }
        };
        let last = batch.last;
        answer.batches.push(batch);
        if last {
            return Ok(answer);
        }
        client.send_on(stream, &Credit::new(1)).await?;
    }
}

/// The next frame of `client`, a batch or a refusal.
pub async fn next_of(client: &mut EndpointClient) -> TestResult<Result<Batch, Refused>> {
    let frame = client.next_frame("a batch or a refusal").await?;
    match refusal_of(&frame)? {
        Some(refused) => Ok(Err(refused)),
        None => Ok(Ok(batch_of(&frame)?)),
    }
}
