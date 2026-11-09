/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `client_bench`'s run: requests through the three binaries with the real client (task 12.5b). Every connection is the
//! compio shell's `Client` over TCP, logged in as the bootstrap administrator, on the run's one runtime, sending its
//! requests one after another, each timed from its call to its answer: a closed loop, so a connection's rate is the
//! inverse of its latency and the run's is the sum over its connections. A read run first writes each connection's
//! subjects and waits until a read sees its last one, a span run each connection's rare subjects, a hot run the hot
//! subject's predicates; the clock starts once every connection is ready. A stall run is no loop: each connection
//! pipelines its Writes in one transaction, timing each until its bytes have left, and holds them to the run's end.

use std::collections::BTreeMap;
use std::time::Duration;

use futures::future::join_all;
use ultragraphdb_client_compio::{
    Client, ClientConfig, ClientError, ClientErrorEnum, Session, TcpConnector,
};
use ultragraphdb_protocol_scram::{ScramError, ScramLogin, system_nonce};
use ultragraphdb_protocol_wire::{Begin, BodyBuilder, DirectWrite, Read, ReadLevel, Write};

use crate::errors::bench_error::BenchError;
use crate::traits::clock::Clock;
use crate::types::client_args::ClientArgs;
use crate::types::client_op::ClientOp;
use crate::types::client_shape::{CLIENT_TENANT, ClientShape};
use crate::types::summary::Summary;
use crate::types::wall_clock::WallClock;

/// The administrator the frontend's first start creates from its bootstrap secret.
const ADMIN: &str = "admin";
/// The name this client says hello with.
const CLIENT_NAME: &str = "client_bench";
/// The frame buffers each connection holds: a read opens only while the pool holds what the Welcome's credit can bring,
/// so room for two reads at that credit, as the process tests give their clients.
const BUFFERS: usize = 16;
/// The most requests a connection sends in each second of a run, past any rate a closed loop reaches.
const REQUESTS_PER_SECOND_MAX: u64 = 1_000_000;
/// How many reads of its last subject a read run's connection makes at most before its timed reads, each at a newer
/// snapshot, [`READY_PAUSE`] apart.
const READY_READS: u32 = 500;
const READY_PAUSE: Duration = Duration::from_millis(20);
/// How many resends resolve a commit in doubt at most, [`RESOLVE_PAUSE`] apart: a minute, longer than a partition takes to
/// restart and recover.
const RESOLVE_ATTEMPTS: u32 = 600;
const RESOLVE_PAUSE: Duration = Duration::from_millis(100);

type BenchLogin = ScramLogin<fn() -> Result<String, ScramError>>;
type BenchClient = Client<TcpConnector, BenchLogin>;

/// One connection of the run: its client, its session when commits go through one, its commits, and the subjects it
/// wrote for its reads.
struct Connection {
    client: BenchClient,
    session: Option<Session>,
    shape: ClientShape,
    written: Vec<String>,
}

/// What one connection counted over the timed part of the run.
#[derive(Debug, Default)]
struct Tally {
    /// Each request's time, from its call to its answer, in microseconds; a stall run's Writes, until their bytes left.
    request_us: Vec<u64>,
    /// Each transaction's Begin, from its call to its answer, in microseconds.
    begin_us: Vec<u64>,
    refused: u64,
    /// The commits left in doubt and then resolved by a resend.
    in_doubt: u64,
    first_refusal: Option<String>,
    /// The refusals by their code and detail, `Conflict/1` say.
    refusals: BTreeMap<String, u64>,
    rows: u64,
    empty: u64,
    /// The body bytes of the Writes a stall run sent.
    sent_bytes: u64,
}

/// Runs `client_bench`'s `args` and returns its report, one JSON line.
///
/// # Errors
///
/// [`BenchError::io`] when the password file cannot be read; [`BenchError::client`] when a connection fails to open, or
/// a request fails otherwise than by a refusal, which the report counts; what [`ClientShape::new`] refuses.
pub fn run_client(args: &ClientArgs) -> Result<String, BenchError> {
    let password = std::fs::read_to_string(args.password_file())
        .map_err(|error| BenchError::io(format!("{}: {error}", args.password_file().display())))?;
    // A draw from the system's random source in every name, so two runs against one cluster write distinct nodes: a process ID
    // does not tell runs apart, as every run in a container of its own is process 1.
    let run = system_nonce().map_err(|error| BenchError::io(error.to_string()))?;
    let tag = format!("{}-{run}", args.seed());
    let runtime = compio::runtime::Runtime::new()?;
    runtime.block_on(run_connections(args, &password, &tag))
}

/// Opens every connection, readies it, then times them together from one clock.
///
/// Cancellation: the run's future is driven to its end by [`run_client`]; dropped, it closes every connection.
async fn run_connections(
    args: &ClientArgs,
    password: &str,
    tag: &str,
) -> Result<String, BenchError> {
    let mut connections = Vec::new();
    for connection in 0..args.connections() {
        connections.push(open(args, password, tag, connection).await?);
    }
    for connection in &mut connections {
        connection.ready(args).await?;
    }
    for connection in &mut connections {
        connection.until_readable(args).await?;
    }
    let clock = WallClock::new();
    let end = Duration::from_secs(args.seconds());
    let bound = args.seconds().saturating_mul(REQUESTS_PER_SECOND_MAX);
    let runs = connections
        .iter_mut()
        .map(|connection| connection.timed(args, &clock, (end, bound)));
    let tallies = join_all(runs)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let elapsed = clock.elapsed();
    for connection in &mut connections {
        connection.client.close().await;
    }
    Ok(report(args, &tallies, elapsed))
}

/// Connection `connection` of the run `tag`, connected and logged in, its session registered when `args` asks for one.
async fn open(
    args: &ClientArgs,
    password: &str,
    tag: &str,
    connection: u64,
) -> Result<Connection, BenchError> {
    let nonces: fn() -> Result<String, ScramError> = system_nonce;
    let login = ScramLogin::new(ADMIN, password, nonces)
        .map_err(|error| BenchError::client(error.to_string()))?;
    let config = ClientConfig::new(args.cluster(), CLIENT_NAME);
    let mut client = Client::new(TcpConnector::new(args.address()), config, login, BUFFERS);
    client
        .connect()
        .await
        .map_err(|error| BenchError::client(format!("{}: {error}", args.address())))?;
    let session = if args.session() {
        Some(client.register().await.map_err(|error| failed(&error))?)
    } else {
        None
    };
    let shape = ClientShape::new(
        tag,
        connection,
        args.relations(),
        (args.partitions(), args.hubs()),
        args.seed(),
    )?
    .with_value_bytes(args.value_bytes());
    Ok(Connection {
        client,
        session,
        shape,
        written: Vec::new(),
    })
}

impl Connection {
    /// Writes what the op's timed part reads or writes on: a read run's subjects, a span run's rare subjects, a hot run's
    /// predicates of the hot subject.
    async fn ready(&mut self, args: &ClientArgs) -> Result<(), BenchError> {
        match args.op() {
            ClientOp::Read => self.write_subjects(args).await,
            ClientOp::Span => {
                for write in self.shape.span_writes()? {
                    self.commit_direct(write).await?;
                }
                Ok(())
            }
            ClientOp::ListRead | ClientOp::Hot => {
                self.commit_direct(ClientShape::hot_seed(args.predicates())?)
                    .await
            }
            ClientOp::Direct | ClientOp::Transaction | ClientOp::Scan | ClientOp::Stall => Ok(()),
        }
    }

    /// Commits `write` through the connection's session.
    async fn commit_direct(&mut self, write: DirectWrite) -> Result<(), BenchError> {
        let _committed = self
            .client
            .direct_write(write, self.session)
            .await
            .map_err(|error| failed(&error))?;
        Ok(())
    }

    /// Writes at least `args.subjects()` subjects by DirectWrite, for the reads that follow.
    async fn write_subjects(&mut self, args: &ClientArgs) -> Result<(), BenchError> {
        let commits = args.subjects().div_ceil(args.relations() as u64);
        for _ in 0..commits {
            let write = self.shape.next_direct()?;
            self.commit_direct(write).await?;
            self.written.extend_from_slice(self.shape.subjects());
        }
        Ok(())
    }

    /// Reads what a read or span run wrote until a snapshot holds it, so every timed read finds what it reads: a read run's
    /// last subject, a span run's row in each partition.
    async fn until_readable(&mut self, args: &ClientArgs) -> Result<(), BenchError> {
        let (read, rows) = match args.op() {
            ClientOp::Read => match self.written.last() {
                Some(last) => (ClientShape::read_of(last, 0), 1),
                None => return Ok(()),
            },
            ClientOp::Span => (self.shape.span_of(0), u64::from(args.partitions())),
            ClientOp::Direct
            | ClientOp::Transaction
            | ClientOp::Scan
            | ClientOp::Stall
            | ClientOp::ListRead
            | ClientOp::Hot => return Ok(()),
        };
        for _ in 0..READY_READS {
            let (found, _) = self
                .read_drained(&read, ReadLevel::Snapshot)
                .await
                .map_err(|error| failed(&error))?;
            if found >= rows {
                return Ok(());
            }
            compio::time::sleep(READY_PAUSE).await;
        }
        Err(BenchError::client(format!(
            "no snapshot of {READY_READS} read the {rows} rows the run wrote"
        )))
    }

    /// The timed part of the run: a stall run's one transaction, every other run's loop.
    async fn timed(
        &mut self,
        args: &ClientArgs,
        clock: &impl Clock,
        (end, bound): (Duration, u64),
    ) -> Result<Tally, BenchError> {
        if args.op() == ClientOp::Stall {
            return self.stall(args, clock, end).await;
        }
        let mut tally = Tally::default();
        for _ in 0..bound {
            let request = self.next_request(args)?;
            let start = clock.elapsed();
            if start >= end {
                break;
            }
            let outcome = self.send(request, args.level(), clock, &mut tally).await;
            match outcome {
                Ok(()) => tally.request_us.push(micros(clock.elapsed() - start)),
                Err(ClientError(ClientErrorEnum::Refused {
                    code,
                    detail,
                    message,
                    ..
                })) => {
                    tally.refused += 1;
                    *tally
                        .refusals
                        .entry(format!("{code:?}/{detail}"))
                        .or_default() += 1;
                    let _first = tally
                        .first_refusal
                        .get_or_insert_with(|| format!("{code:?}: {message}"));
                }
                Err(other) => return Err(failed(&other)),
            }
        }
        Ok(tally)
    }

    /// Sends `request` and waits for its answer, a read's at `level`.
    async fn send(
        &mut self,
        request: Request,
        level: ReadLevel,
        clock: &impl Clock,
        tally: &mut Tally,
    ) -> Result<(), ClientError> {
        match request {
            Request::Direct(write) => {
                let written = self.client.direct_write(write, self.session).await;
                match (written, self.session) {
                    (Err(error), Some(session)) if in_doubt(&error) => {
                        tally.in_doubt += 1;
                        self.resolve(session).await
                    }
                    (written, _) => written.map(|_| ()),
                }
            }
            Request::Transaction(write) => self.transaction(None, &write, clock, tally).await,
            Request::ListRead(read, write) => {
                self.transaction(Some(&read), &write, clock, tally).await
            }
            Request::Read(read) => {
                let (rows, begun) = self.read_drained(&read, level).await?;
                tally.begin_us.push(micros(begun));
                tally.rows += rows;
                tally.empty += u64::from(rows == 0);
                Ok(())
            }
        }
    }

    /// Resolves `session`'s commit in doubt as a client does, by resending it with its number until the server answers it
    /// committed or refused: so a commit cut off by a partition's restart is counted once, as acknowledged or refused.
    async fn resolve(&mut self, session: Session) -> Result<(), ClientError> {
        let mut attempt = 1;
        loop {
            compio::time::sleep(RESOLVE_PAUSE).await;
            match self.client.resolve(session).await {
                Err(error) if attempt < RESOLVE_ATTEMPTS && in_doubt(&error) => attempt += 1,
                resolved => return resolved.map(|_| ()),
            }
        }
    }

    /// The next request of `args`'s op: the next commit, a subject this connection wrote, drawn at random, the cluster's
    /// every `w`, this connection's rare predicate, the hot subject read before a new node, or a new value of it.
    fn next_request(&mut self, args: &ClientArgs) -> Result<Request, BenchError> {
        Ok(match args.op() {
            ClientOp::Direct | ClientOp::Stall => Request::Direct(self.shape.next_direct()?),
            ClientOp::Transaction => Request::Transaction(self.shape.next_write()?),
            ClientOp::Read => {
                let held = self.written.len() as u64;
                // Below the subjects' count, which fits a `usize`.
                let at = self.shape.draw_below(held) as usize;
                let subject = self.written.get(at).map_or("", String::as_str);
                Request::Read(ClientShape::read_of(subject, 0))
            }
            ClientOp::Scan => Request::Read(ClientShape::scan_of(0)),
            ClientOp::Span => Request::Read(self.shape.span_of(0)),
            ClientOp::ListRead => {
                let (table, changes) = self.shape.changes_in(self.shape.hot_partition()?)?;
                Request::ListRead(
                    ClientShape::hot_read(0, args.limit()),
                    Write::new(table, changes),
                )
            }
            ClientOp::Hot => Request::Direct(self.shape.next_hot(args.predicates(), args.fresh())?),
        })
    }

    /// One read-write transaction: Begin, `read` drained when there is one, `write`, and Commit; the Begin's time noted.
    async fn transaction(
        &mut self,
        read: Option<&Read>,
        write: &Write,
        clock: &impl Clock,
        tally: &mut Tally,
    ) -> Result<(), ClientError> {
        let start = clock.elapsed();
        let transaction = self.client.begin(CLIENT_TENANT, false).await?;
        tally.begin_us.push(micros(clock.elapsed() - start));
        if let Some(read) = read {
            let read = in_transaction(read, transaction.stream());
            let stream = self.client.read(&transaction, &read).await?;
            while let Some(rows) = self
                .client
                .next_batch(&stream, |batch| u64::from(batch.rows()))
                .await?
            {
                tally.rows += rows;
            }
        }
        self.client.write(&transaction, write).await?;
        let _committed = self.client.commit(transaction, self.session).await?;
        Ok(())
    }

    /// Reads `read` whole in a read-only transaction of its own at `level`, then aborts it: the rows read, and how long
    /// its Begin took.
    async fn read_drained(
        &mut self,
        read: &Read,
        level: ReadLevel,
    ) -> Result<(u64, Duration), ClientError> {
        let begin = WallClock::new();
        let transaction = self
            .client
            .begin_with(Begin::new(CLIENT_TENANT, 0, true).with_level(level), None)
            .await?;
        let begun = begin.elapsed();
        let read = in_transaction(read, transaction.stream());
        let stream = self.client.read(&transaction, &read).await?;
        let mut rows = 0u64;
        while let Some(batch) = self
            .client
            .next_batch(&stream, |batch| u64::from(batch.rows()))
            .await?
        {
            rows += batch;
        }
        self.client.abort(transaction).await?;
        Ok((rows, begun))
    }

    /// A stall run's connection: Begin, then `args.writes()` Writes sent one after another without waiting for their
    /// answers, each timed until its bytes have left, which stops once the frontend stops reading the connection; then held
    /// to `end` and aborted. A Write whose bytes have not left by `end` is given up, and the connection with it.
    ///
    /// Cancellation: a Write given up at `end` may leave part of its frame unsent, so the connection is closed after it
    /// and sends nothing more.
    async fn stall(
        &mut self,
        args: &ClientArgs,
        clock: &impl Clock,
        end: Duration,
    ) -> Result<Tally, BenchError> {
        let mut tally = Tally::default();
        let begun = clock.elapsed();
        let transaction = self
            .client
            .begin(CLIENT_TENANT, false)
            .await
            .map_err(|error| failed(&error))?;
        tally.begin_us.push(micros(clock.elapsed() - begun));
        // The hot subject's partition, so a run of several partitions stalls one named partition.
        let partition = self.shape.hot_partition()?;
        let mut whole = true;
        for _ in 0..args.writes() {
            let (table, changes) = self.shape.changes_in(partition)?;
            let write = Write::new(table, changes);
            let start = clock.elapsed();
            let left = end.saturating_sub(start);
            match compio::time::timeout(left, self.client.write(&transaction, &write)).await {
                Ok(sent) => sent.map_err(|error| failed(&error))?,
                Err(_) => {
                    whole = false;
                    break;
                }
            }
            tally.request_us.push(micros(clock.elapsed() - start));
            tally.sent_bytes += write.body().len() as u64;
        }
        compio::time::sleep(end.saturating_sub(clock.elapsed())).await;
        if whole {
            // A transaction past its limit is ended already, so an Abort finding it gone is no failure of the run.
            let _aborted: Result<(), ClientError> = self.client.abort(transaction).await;
        }
        Ok(tally)
    }
}

/// One request of a run, built before its time starts.
enum Request {
    Direct(DirectWrite),
    Transaction(Write),
    /// A read-write transaction's read, then its write.
    ListRead(Read, Write),
    /// A read-only transaction's read.
    Read(Read),
}

/// `read` in the transaction of stream `transaction`.
fn in_transaction(read: &Read, transaction: u64) -> Read {
    Read::new(
        transaction,
        read.limit(),
        read.terms().clone(),
        *read.pattern(),
        read.resume().to_vec(),
    )
}

/// `elapsed` in whole microseconds.
fn micros(elapsed: Duration) -> u64 {
    u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX)
}

/// Whether `error` leaves a commit in doubt.
fn in_doubt(error: &ClientError) -> bool {
    matches!(error.0, ClientErrorEnum::InDoubt { .. })
}

/// A request's failure as the run's.
fn failed(error: &ClientError) -> BenchError {
    BenchError::client(error.to_string())
}

/// The report of a run: the requests answered and refused, the rates, and the latencies over every connection.
fn report(args: &ClientArgs, tallies: &[Tally], elapsed: Duration) -> String {
    let secs = elapsed.as_secs_f64().max(f64::MIN_POSITIVE);
    let mut request_us: Vec<u64> = tallies
        .iter()
        .flat_map(|t| t.request_us.iter().copied())
        .collect();
    let mut begin_us: Vec<u64> = tallies
        .iter()
        .flat_map(|t| t.begin_us.iter().copied())
        .collect();
    let requests = request_us.len() as u64;
    let refused: u64 = tallies.iter().map(|t| t.refused).sum();
    let in_doubt: u64 = tallies.iter().map(|t| t.in_doubt).sum();
    let relations = match args.op() {
        ClientOp::Direct | ClientOp::Transaction | ClientOp::ListRead | ClientOp::Stall => {
            requests * args.relations() as u64
        }
        ClientOp::Hot => requests,
        ClientOp::Read | ClientOp::Scan | ClientOp::Span => 0,
    };
    let mut refusals: BTreeMap<&str, u64> = BTreeMap::new();
    for (kind, count) in tallies.iter().flat_map(|t| t.refusals.iter()) {
        *refusals.entry(kind.as_str()).or_default() += count;
    }
    let refusals = refusals
        .iter()
        .map(|(kind, count)| format!("\"{kind}\":{count}"))
        .collect::<Vec<_>>()
        .join(",");
    let first_refusal = tallies
        .iter()
        .find_map(|t| t.first_refusal.as_deref())
        .unwrap_or("")
        .replace(['"', '\\'], "'");
    format!(
        concat!(
            "{{\"run\":\"client\",\"op\":\"{}\",\"connections\":{},\"partitions\":{},\"relations_per_commit\":{},",
            "\"session\":{},\"level\":{},\"limit\":{},\"seconds\":{:.2},\"requests\":{},\"refused\":{},\"in_doubt\":{},",
            "\"refusals\":{{{}}},\"requests_per_s\":{:.0},\"relations_per_s\":{:.0},\"latency_us\":{},",
            "\"begin_us\":{},\"rows\":{},\"empty_reads\":{},\"sent_bytes\":{},\"first_refusal\":\"{}\"}}"
        ),
        args.op().name(),
        args.connections(),
        args.partitions(),
        args.relations(),
        args.session(),
        args.level().number(),
        args.limit(),
        elapsed.as_secs_f64(),
        requests,
        refused,
        in_doubt,
        refusals,
        requests as f64 / secs,
        relations as f64 / secs,
        summary_json(&Summary::of(&mut request_us)),
        summary_json(&Summary::of(&mut begin_us)),
        tallies.iter().map(|t| t.rows).sum::<u64>(),
        tallies.iter().map(|t| t.empty).sum::<u64>(),
        tallies.iter().map(|t| t.sent_bytes).sum::<u64>(),
        first_refusal,
    )
}

/// `summary` as a JSON object.
fn summary_json(summary: &Summary) -> String {
    format!(
        "{{\"p50\":{},\"p90\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}}",
        summary.p50(),
        summary.p90(),
        summary.p99(),
        summary.max(),
        summary.mean()
    )
}
