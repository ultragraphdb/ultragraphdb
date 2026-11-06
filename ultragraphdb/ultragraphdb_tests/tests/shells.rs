/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The client of these scenarios through either shell, over TLS or clear text: the client crates' own `Client`, one type for a
//! step to speak whichever a run takes, and that shell's runtime and timers.

use std::cell::{OnceCell, RefCell};
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::future::{Either, select};
use ultragraphdb_client_compio::{
    ClientConfig, ClientError, Committed, Connect, ResultStream, Session, Statement, TlsConnector,
    Transaction,
};
use ultragraphdb_core_ids::SessionToken;
use ultragraphdb_protocol_scram::{ScramError, ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::{Address, Scheme};
use ultragraphdb_protocol_transport_compio::Carrier;
use ultragraphdb_protocol_wire::{
    Begin, ConditionalWrite, Describe, DirectWrite, DropCommand, Execute, Operation, Prepare, Read,
    ResultView, Welcome, Write,
};
use ultragraphdb_tests::CLUSTER;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// How long one step's work may take in all: past every limit and timeout a scenario meets, so a client that never answers
/// fails its step naming what it waited for.
const STEP_WAIT: Duration = Duration::from_secs(60);

/// The administrator the frontend's first start creates, whom every client of these scenarios logs in as.
pub(crate) const ADMIN: &str = "admin";

/// The administrator's password, which the frontend's bootstrap secret holds.
pub(crate) const PASSWORD: &str = "the administrator's password";

/// The name this crate's clients say hello with.
const CLIENT_NAME: &str = "ultragraphdb-tests";

/// The hostname the test PKI's certificate names.
const TLS_HOST: &str = "localhost";

/// The frame buffers each client holds, room for two reads at the Welcome's credit beside each other.
const BUFFERS: usize = 16;

/// Which client shell a run drives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Shell {
    /// `ultragraphdb_client_compio`.
    #[default]
    Compio,
    /// `ultragraphdb_client_tokio`.
    Tokio,
}

/// Whether this run's clients take the tokio shell, as the tokio passes' do.
static ON_TOKIO: AtomicBool = AtomicBool::new(false);

impl Shell {
    /// The shell this run drives.
    pub(crate) fn current() -> Self {
        if ON_TOKIO.load(Ordering::Relaxed) {
            Self::Tokio
        } else {
            Self::Compio
        }
    }

    /// Makes this the shell of the run that follows.
    pub(crate) fn select(self) {
        ON_TOKIO.store(self == Self::Tokio, Ordering::Relaxed);
    }
}

/// Waits `duration` on the current shell's timer.
pub(crate) async fn pause(duration: Duration) {
    match Shell::current() {
        Shell::Compio => compio::time::sleep(duration).await,
        Shell::Tokio => tokio::time::sleep(duration).await,
    }
}

/// `work`'s outcome, or a failure naming `what` it waited for once `bound` has passed without one.
pub(crate) async fn within<T>(
    what: &str,
    bound: Duration,
    work: impl Future<Output = TestResult<T>>,
) -> TestResult<T> {
    let work = pin!(work);
    let timer = pin!(pause(bound));
    match select(work, timer).await {
        Either::Left((outcome, _)) => outcome,
        Either::Right(_) => Err(TestError::new(format!(
            "{what} did not finish within {bound:?}"
        ))),
    }
}

thread_local! {
    /// The compio runtime of this thread's steps, which a client kept between them runs on.
    static COMPIO: OnceCell<compio::runtime::Runtime> = const { OnceCell::new() };
    /// The tokio runtime of this thread's steps.
    static TOKIO: OnceCell<tokio::runtime::Runtime> = const { OnceCell::new() };
    /// Clients kept between this thread's steps, each with where it dials.
    static KEPT: RefCell<Vec<(String, TestClient)>> = const { RefCell::new(Vec::new()) };
}

/// Runs a step's `work` on this thread's runtime of the current shell, failing past [`STEP_WAIT`].
pub(crate) fn run<T>(work: impl Future<Output = TestResult<T>>) -> TestResult<T> {
    let bounded = within("the step's work", STEP_WAIT, work);
    match Shell::current() {
        Shell::Compio => COMPIO.with(|runtime| {
            if runtime.get().is_none() {
                drop(runtime.set(compio::runtime::Runtime::new()?));
            }
            runtime.get().context("a compio runtime")?.block_on(bounded)
        }),
        Shell::Tokio => TOKIO.with(|runtime| {
            if runtime.get().is_none() {
                let built = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                drop(runtime.set(built));
            }
            runtime.get().context("a tokio runtime")?.block_on(bounded)
        }),
    }
}

/// Runs `scene` on a client of `at`, kept between the steps of this thread: a login derives 600,000 iterations of PBKDF2, and the
/// scenarios of a shared cluster do not disturb each other's connection. A scene that needs a connection of its own dials one.
pub(crate) fn with_client<T>(
    at: &ClientAt,
    scene: impl AsyncFnOnce(&mut TestClient) -> TestResult<T>,
) -> TestResult<T> {
    run(async {
        let mut client = borrowed(at).await?;
        let outcome = scene(&mut client).await;
        keep(at, client);
        outcome
    })
}

/// A client of `at`, the one kept if any: [`keep`] gives it back.
pub(crate) async fn borrowed(at: &ClientAt) -> TestResult<TestClient> {
    let held = KEPT.with_borrow_mut(|kept| {
        kept.iter()
            .position(|(address, _)| *address == at.address)
            .map(|place| kept.swap_remove(place).1)
    });
    match held {
        Some(client) => Ok(client),
        None => crate::cluster::client(at).await,
    }
}

/// Keeps `client` of `at` for a later step.
pub(crate) fn keep(at: &ClientAt, client: TestClient) {
    KEPT.with_borrow_mut(|kept| kept.push((at.address.clone(), client)));
}

/// Lets go of the clients kept, on the runtime they ran on, before the clusters they dial stop.
pub(crate) fn forget_clients() -> TestResult {
    run(async {
        drop(KEPT.take());
        Ok(())
    })
}

/// Lets go of the clients kept for `address`, on their runtime.
pub(crate) fn forget_clients_of(address: &str) {
    let gone: Vec<(String, TestClient)> = KEPT.with_borrow_mut(|kept| {
        let (gone, stay) = std::mem::take(kept)
            .into_iter()
            .partition(|(held, _)| held == address);
        *kept = stay;
        gone
    });
    if !gone.is_empty() {
        // A runtime that cannot be entered lets go of them with the process.
        drop(run(async {
            drop(gone);
            Ok(())
        }));
    }
}

/// Where a client dials: an address, and the trust to verify it by when it speaks TLS.
#[derive(Debug, Clone)]
pub(crate) struct ClientAt {
    pub(crate) address: String,
    pub(crate) tls: Option<TlsConnector>,
}

impl ClientAt {
    /// The address as this crate's clients dial it over clear text.
    pub(crate) fn clear_text(address: &str) -> Self {
        Self {
            address: address.to_string(),
            tls: None,
        }
    }
}

/// Who a client logs in as and how it retries; [`Knobs::default`] is the administrator with the shell's own defaults.
#[derive(Debug, Clone)]
pub(crate) struct Knobs {
    pub(crate) name: String,
    pub(crate) password: String,
    pub(crate) commit_attempts: Option<u32>,
    pub(crate) timeout: Option<Duration>,
}

impl Default for Knobs {
    fn default() -> Self {
        Self {
            name: ADMIN.to_string(),
            password: PASSWORD.to_string(),
            commit_attempts: None,
            timeout: None,
        }
    }
}

impl Knobs {
    /// The administrator's client that takes one attempt at each commit, so a lost reply is the caller's to see.
    pub(crate) fn one_attempt() -> Self {
        Self {
            commit_attempts: Some(1),
            ..Self::default()
        }
    }

    /// The administrator's client of timeout `timeout`.
    pub(crate) fn timing_out(timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            ..Self::default()
        }
    }

    /// A client logging in as `name` with `password`.
    pub(crate) fn logging_in(name: &str, password: &str) -> Self {
        Self {
            name: name.to_string(),
            password: password.to_string(),
            ..Self::default()
        }
    }

    fn config(&self) -> ClientConfig {
        let config = ClientConfig::new(CLUSTER, CLIENT_NAME);
        let config = match self.commit_attempts {
            Some(attempts) => config.with_commit_attempts(attempts),
            None => config,
        };
        match self.timeout {
            Some(timeout) => config.with_timeout(timeout),
            None => config,
        }
    }

    fn login(&self) -> TestResult<Login> {
        let nonces: fn() -> Result<String, ScramError> = system_nonce;
        Ok(ScramLogin::new(&self.name, &self.password, nonces)?)
    }
}

/// How these scenarios log in.
type Login = ScramLogin<fn() -> Result<String, ScramError>>;

/// Connections through the compio shell to one address, over the carrier its scheme names.
#[derive(Debug, Clone)]
pub(crate) struct CompioDial(Address);

impl CompioDial {
    /// Connections to `address`.
    pub(crate) fn of(address: Address) -> Self {
        Self(address)
    }
}

impl Connect for CompioDial {
    type Stream = Carrier;

    async fn connect(&mut self) -> io::Result<Carrier> {
        Carrier::connect(&self.0)
            .await
            .map_err(|error| io::Error::other(error.to_string()))
    }
}

/// The address the tokio shell dials, which speaks TCP alone.
fn tokio_target(address: &Address) -> TestResult<SocketAddr> {
    match address.scheme() {
        Scheme::Tcp => Ok(address.target().parse()?),
        Scheme::Unix => Err(TestError::new(
            "the tokio shell's connector dials TCP; its passes take TCP and TLS",
        )),
    }
}

/// A client through one shell, over TLS or clear text.
#[derive(Debug)]
pub(crate) enum TestClient {
    Compio(Box<ultragraphdb_client_compio::Client<CompioDial, Login>>),
    CompioTls(
        Box<
            ultragraphdb_client_compio::Client<
                ultragraphdb_client_compio::TlsDial<CompioDial>,
                Login,
            >,
        >,
    ),
    Tokio(Box<ultragraphdb_client_tokio::Client<ultragraphdb_client_tokio::TcpConnector, Login>>),
    TokioTls(
        Box<
            ultragraphdb_client_tokio::Client<
                ultragraphdb_client_tokio::TlsDial<ultragraphdb_client_tokio::TcpConnector>,
                Login,
            >,
        >,
    ),
}

/// Runs `$call` on the client whichever shell it is: the shells' methods are one surface over two types, and no trait spans
/// them.
macro_rules! on_shell {
    ($client:expr, $shell:ident => $call:expr) => {
        match $client {
            TestClient::Compio($shell) => $call,
            TestClient::CompioTls($shell) => $call,
            TestClient::Tokio($shell) => $call,
            TestClient::TokioTls($shell) => $call,
        }
    };
}

impl TestClient {
    /// A client of `at` through the current shell, not yet connected.
    pub(crate) fn new(at: &ClientAt, knobs: &Knobs) -> TestResult<Self> {
        let address = Address::parse(&at.address)?;
        let config = knobs.config();
        let login = knobs.login()?;
        Ok(match (Shell::current(), &at.tls) {
            (Shell::Compio, None) => {
                Self::Compio(Box::new(ultragraphdb_client_compio::Client::new(
                    CompioDial(address),
                    config,
                    login,
                    BUFFERS,
                )))
            }
            (Shell::Compio, Some(tls)) => {
                Self::CompioTls(Box::new(ultragraphdb_client_compio::Client::new(
                    ultragraphdb_client_compio::TlsDial::new(
                        CompioDial(address),
                        tls.clone(),
                        TLS_HOST,
                    ),
                    config,
                    login,
                    BUFFERS,
                )))
            }
            (Shell::Tokio, None) => Self::Tokio(Box::new(ultragraphdb_client_tokio::Client::new(
                ultragraphdb_client_tokio::TcpConnector::new(tokio_target(&address)?),
                config,
                login,
                BUFFERS,
            ))),
            (Shell::Tokio, Some(tls)) => {
                Self::TokioTls(Box::new(ultragraphdb_client_tokio::Client::new(
                    ultragraphdb_client_tokio::TlsDial::new(
                        ultragraphdb_client_tokio::TcpConnector::new(tokio_target(&address)?),
                        tls.clone(),
                        TLS_HOST,
                    ),
                    config,
                    login,
                    BUFFERS,
                )))
            }
        })
    }

    /// Opens a connection and authenticates.
    pub(crate) async fn connect(&mut self) -> Result<(), ClientError> {
        on_shell!(self, client => client.connect().await)
    }

    /// The Welcome of the connection.
    pub(crate) fn welcome(&self) -> Option<&Welcome> {
        on_shell!(self, client => client.core().welcome())
    }

    /// The principal the connection authenticated as.
    pub(crate) fn principal(&self) -> Option<u64> {
        on_shell!(self, client => client.core().principal())
    }

    /// The token `session` holds.
    pub(crate) fn session_token(&self, session: Session) -> Option<&SessionToken> {
        on_shell!(self, client => client.core().session_token(session))
    }

    /// Registers a session.
    pub(crate) async fn register(&mut self) -> Result<Session, ClientError> {
        on_shell!(self, client => client.register().await)
    }

    /// A session kept from an earlier client.
    pub(crate) fn resume_session(
        &mut self,
        number: u128,
        next_request: u64,
    ) -> Result<Session, ClientError> {
        on_shell!(self, client => client.resume_session(number, next_request))
    }

    /// Begins a transaction at the default level.
    pub(crate) async fn begin(
        &mut self,
        tenant: u64,
        read_only: bool,
    ) -> Result<Transaction, ClientError> {
        on_shell!(self, client => client.begin(tenant, read_only).await)
    }

    /// Begins the transaction `begin` names.
    pub(crate) async fn begin_with(
        &mut self,
        begin: Begin,
        session: impl Into<Option<Session>>,
    ) -> Result<Transaction, ClientError> {
        on_shell!(self, client => client.begin_with(begin, session).await)
    }

    /// Writes a changeset.
    pub(crate) async fn write(
        &mut self,
        transaction: &Transaction,
        write: &Write,
    ) -> Result<(), ClientError> {
        on_shell!(self, client => client.write(transaction, write).await)
    }

    /// Asks for a commit-time operation.
    pub(crate) async fn operation(
        &mut self,
        transaction: &Transaction,
        operation: &Operation,
    ) -> Result<(), ClientError> {
        on_shell!(self, client => client.operation(transaction, operation).await)
    }

    /// Aborts a transaction.
    pub(crate) async fn abort(&mut self, transaction: Transaction) -> Result<(), ClientError> {
        on_shell!(self, client => client.abort(transaction).await)
    }

    /// Commits a transaction.
    pub(crate) async fn commit(
        &mut self,
        transaction: Transaction,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        on_shell!(self, client => client.commit(transaction, session).await)
    }

    /// Writes in one frame, unconditionally.
    pub(crate) async fn direct_write(
        &mut self,
        write: DirectWrite,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        on_shell!(self, client => client.direct_write(write, session).await)
    }

    /// Writes in one frame, with commit-time operations.
    pub(crate) async fn conditional_write(
        &mut self,
        write: ConditionalWrite,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        on_shell!(self, client => client.conditional_write(write, session).await)
    }

    /// Drops what `command` names.
    pub(crate) async fn drop_command(
        &mut self,
        command: DropCommand,
        session: impl Into<Option<Session>>,
    ) -> Result<Committed, ClientError> {
        on_shell!(self, client => client.drop_command(command, session).await)
    }

    /// Resolves `session`'s request in doubt by sending it again.
    pub(crate) async fn resolve(&mut self, session: Session) -> Result<Committed, ClientError> {
        on_shell!(self, client => client.resolve(session).await)
    }

    /// Prepares a statement.
    pub(crate) async fn prepare(&mut self, prepare: &Prepare) -> Result<Statement, ClientError> {
        on_shell!(self, client => client.prepare(prepare).await)
    }

    /// `tenant`'s data dictionary.
    pub(crate) async fn describe(&mut self, tenant: u64) -> Result<Describe, ClientError> {
        on_shell!(self, client => client.describe(tenant).await)
    }

    /// Reads a quad pattern: a result stream.
    pub(crate) async fn read(
        &mut self,
        transaction: &Transaction,
        read: &Read,
    ) -> Result<ResultStream, ClientError> {
        on_shell!(self, client => client.read(transaction, read).await)
    }

    /// Executes a prepared statement: a result stream.
    pub(crate) async fn execute(
        &mut self,
        transaction: &Transaction,
        execute: &Execute,
    ) -> Result<ResultStream, ClientError> {
        on_shell!(self, client => client.execute(transaction, execute).await)
    }

    /// The next batch of `stream`, as `read` takes it; `None` after the last.
    pub(crate) async fn next_batch<T>(
        &mut self,
        stream: &ResultStream,
        read: impl FnMut(&ResultView<'_>) -> T,
    ) -> Result<Option<T>, ClientError> {
        on_shell!(self, client => client.next_batch(stream, read).await)
    }

    /// Cancels a read.
    pub(crate) async fn cancel(&mut self, stream: ResultStream) {
        on_shell!(self, client => client.cancel(stream).await);
    }

    /// Closes the connection.
    pub(crate) async fn close(&mut self) {
        on_shell!(self, client => client.close().await);
    }
}
