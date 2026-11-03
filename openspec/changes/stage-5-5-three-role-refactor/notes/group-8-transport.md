# Group 8: transport (`protocol_transport`, `protocol_transport_compio`, `transport_tls`)

The transport half of group 8 (wave 5, agent "transport"): 8.1 for the three crates, the transport part of 8.3–8.5,
and every transport-tls scenario. The wire half (`protocol_wire`) is the wire agent's (`group-8-wire.md`, when it
lands). Scratch files are in the session scratchpad under `g8t/` (`lift.diff`, the mutants logs).

## 8.1 The Ultra Style lift, each crate its own step

Bazel before the lift: `protocol_transport` unit 116; `protocol_transport_compio` unit 18, bdd 18 scenarios;
`transport_tls` unit 22, bdd 6 scenarios, doc test 1. `cargo fmt` changed nothing; `cargo clippy --all-targets` with the
workspace table reported nothing. With the four lints the table holds back (`indexing_slicing`,
`arithmetic_side_effects`, `cast_possible_truncation`, `cast_sign_loss`) the three crates' `src/` showed 26 findings;
those in code that reads input from outside were fixed, the rest reviewed:

- **Rule 7, arithmetic on values from outside.** `ServerConnection` added the configured limits and a credential's
  validity to an `Instant` with `+`, which panics past what an `Instant` holds (`with_limits(Duration::MAX, …)`); it
  now uses `checked_add`, and a limit or validity past that range sets no deadline
  (`test_limits_past_what_an_instant_holds_set_no_deadline`). `ReceivedFrame::field`'s `at + N` is `checked_add`;
  `Outbound`'s credit decrement is `saturating_sub` with the precondition debug-checked; compio's `fill` reserves with
  `saturating_sub`; the simulated network's counters and clock saturate.
- **Rule 7, panicking slices.** The simulated network cut a message into chunks with `message[start..cut]`; the cut
  moved into `NetworkState::split`, reading through `get`, its draws in the same order, so a seed replays as before.
- **Rule 2, bounded loops.** `NetworkState::step`'s inner `loop` takes at most the link's messages in flight at the step's
  start (none is sent during a step); `SimNetwork::run_until_idle` steps at most `max_delay + 1` times, every message
  being due within `max_delay` steps of its send; compio's `Connection::read_frame` fills at most `FILLS_PER_FRAME` (2)
  times, the header and then the frame, and its `fill` is an event loop, which its comment says; `TestPeer::receive`'s
  `while` says why it ends.
- **Rule 4.** Wire's 19 role `Command` variants (they landed during the lift) pushed `ServerConnection::request` and
  `ClientAuthentication::receive` past 70 lines: the request's command now maps to a `RequestKind`
  (`server_connection/request_kind.rs`) that the stream rules match on, and the Authenticate branch is
  `ClientAuthentication::authenticate`.
- **Rules 8, 9.** `BufferPool`'s `Arc<Mutex<…>>`, the simulated network's `Rc<RefCell<…>>`, the connector's and trust
  store's `Arc` each carry their reason; the one `cfg` is `test-utils`.
- **Reviewed, unchanged.** The remaining casts are lossless by construction (`below(3) as usize`, said so in a comment);
  `split_mix` wraps on purpose.

Bazel after the lift: the same counts. The diff is `g8t/lift.diff` (9 files and `request_kind.rs`). It lands with the
group's changes, not as a commit of its own; if the history should show it apart, the user can commit that diff first.

Note: the lift's compile fix for wire's role commands put them on a `not_served` stub (phase 1), implemented in phase 4.
While saving the lift's diff I ran `git add -N` on `request_kind.rs` once, by mistake, and removed the intent-to-add
entry at once with `git rm --cached`; the index is as it was.

## Phase 1: the surface

- `protocol_transport`: `Address` (`parse`, `scheme`, `target`, `host_port`, `path`, `Display`), `Scheme { Tcp, Unix }`,
  `ADDRESS_LEN_MAX` (1,024), `TransportErrorEnum::Address { address, reason }` with `TransportError::address`;
  `ServerConnection` refusing role commands (`not_served`).
- `protocol_transport_compio`: `Carrier { Tcp, Unix }` (compio `AsyncRead`, `AsyncWrite`) with `Carrier::connect`;
  `Listener { Tcp, Unix }` with `bind`, `accept`, `local_address`; `CompioTlsStream<S>` with `connect`, `accept`,
  `session`, `get_ref`, and its own `AsyncRead`, `AsyncWrite`.
- `transport_tls`: `TlsErrorEnum::{CertificateUnreadable, CertificateEmpty, KeyMismatch, RecordRefused}`;
  `TlsSession` (`receive`, `transmit`, `send`, `read -> Plaintext`, `close`, `is_handshaking`, `wants_write`,
  `wants_read`, `protocol_version`, `host`); `Plaintext { Read, Pending, Closed, Truncated }`; `TlsAcceptor`
  (`from_pem_files`, `from_der`, `session`, `accept`); `TlsConnector::session`; `TlsStream<S>`, now the crate's own
  blocking driver of a session (`session`, `get_ref`, `get_mut`) instead of rustls's `StreamOwned`. Behind `test-utils`:
  `TestPki::{server_chain_pem, server_key_pem, other_key_pem, acceptor, connector, client_tls12}`, `TestPeer::new`
  over any session (client or server), `TestPipe`, `TestLink`.

## Phase 2: the suite, observed failing

Bazel against the surface: `protocol_transport` unit 128, 117 passed, 11 failed (10 address tests and the role-command
test); compio unit 29, 18 passed, 11 failed; `transport_tls` unit 47, 16 passed, 31 failed, bdd 10 scenarios, 3 passed,
7 failed. Every failure was the `not implemented` panic (the count of panics equalled the count of failures in each log).

Corner cases enumerated before the suite:

| Case | Test |
|---|---|
| TCP by name, IPv4, bracketed IPv6, port 0 and 65,535; Unix paths of one and several segments; printing and parsing back; two ports apart | `address_tests` |
| Schemes empty, missing, unknown, upper-case, one slash short; a TCP address empty, without a host, without a port, a bracketed host without a port; ports 65,536, 20 digits, letters, `+80`, `-1`, a space, a path after it; a host holding `/` or `@`; Unix paths empty, relative, the root, ending in `/`; whitespace and control characters, leading and trailing; exactly `ADDRESS_LEN_MAX` and one past | `address_tests` |
| Every role command, 512 to 530, after authentication: closed with code 1, one Error on stream 0 flagged last whose message is `CommandNotServed`'s text, nothing taken after | `test_every_role_command_is_refused_as_not_served_and_closes_the_connection` |
| A session pair through bytes alone: TLS 1.3, the hosts, plaintext queued before the handshake, reads in pieces, `Pending`; a 10-byte transmit buffer; `close_notify` read as `Closed`, a bare close as `Truncated`; bytes that are no TLS at the server and the alert at the client, each side naming its host; a damaged record after the handshake as `RecordRefused`; a TLS 1.2 client refused by both sides | `tls_session_tests` |
| An acceptor from files serving TLS 1.3; an unreadable chain and an unreadable key, each named; empty files, a chain file holding a key, a key file holding a certificate; the other key and another authority's key as `KeyMismatch` naming the key file; DER values, an empty chain, bytes that are no certificate; `accept` before the caller reads, the server speaking first; a TLS 1.2 client refused at `accept`; two sessions of one acceptor | `tls_acceptor_tests` |
| Writes in pieces and a flush; a stream ending without `close_notify` as `UnexpectedEof`; a stream closed during the handshake, client and server; a stream whose reads are always interrupted, refused after exactly `HANDSHAKE_TURNS_MAX` (4,096) reads | `tls_stream_tests` |
| TCP and Unix carriers, bytes both ways and a shutdown read as the end; the system's port for port 0; a Unix path bound twice; a directory that does not exist; dialling where nothing listens; a `Connection`'s Hello and Welcome over a Unix socket; TLS over TCP and over a Unix socket with a line both ways and `close_notify`; a `Connection` inside TLS over a Unix socket; an untrusted CA failing both sides; a host no certificate names | `carrier_tests`, `listener_tests`, `compio_tls_stream_tests` |

## Phase 3: inputs reviewed for coincidences

The key-mismatch tests write the chain and the key to different files, so a refusal naming the wrong file fails. The
acceptor's file tests name the missing file on each side in turn, and the empty-file table names a distinct file in
each row. The TLS 1.2 refusal is checked on both sides with the side's own host (empty on the server). The role-command
test runs every role command, not one, so a classifier keyed on a single number fails. The echo tests send lines of
distinct lengths (4, 5, 8 bytes) through the same helpers.

## Phase 4: implementation

- **Address.** `parse` checks, in order, the length, control characters and whitespace, the scheme, then the target:
  a TCP `host:port` (bracketed IPv6 host, no `/` or `@`, a port of ASCII digits below 65,536, since `u16::from_str` takes
  a leading `+`) or an absolute Unix path naming a file. Each refusal names the address as given and its rule.
- **The client endpoint refuses role commands.** `ServerConnection` is the client endpoint's connection: a role command
  (512–767) on any stream after authentication is refused as `CommandNotServed` naming its number, with an Error on
  stream 0, and the connection closes (`PROTOCOL.md` §2.5 step 3, §3). The role endpoint, where role commands are
  served, is group 9's: a connection's kind is the endpoint it arrives at (§11.12), so group 9's role listener takes
  them with its own connection machine. Before a connection authenticates, a role command is refused as any request is,
  with code 20.
- **Carriers.** `Carrier` and `Listener` are enums over compio's TCP and Unix-domain types, chosen by the address's
  scheme; the frames above them are the same on both (`test_a_connection_runs_its_handshake_over_a_unix_socket`).
  `Listener::bind` creates a Unix socket's file and refuses a path already present; removing a stale socket and the
  file's owner and mode (user 65532, 0600) are group 11's.
- **The TLS session.** `TlsSession` wraps rustls's `Connection` (client or server), bytes in and bytes out: `receive`
  takes what a carrier read (empty meaning the carrier closed), `transmit` fills the caller's buffer and keeps the rest,
  `read` hands out plaintext. A refusal is `HandshakeFailed` while the handshake runs and `RecordRefused` after it, and
  leaves rustls's alert queued for the peer.
- **Drivers.** `TlsStream<S>` (blocking `Read + Write`) and `CompioTlsStream<S>` (compio) run the same loop: each
  handshake turn writes every queued record, then reads once, at most `HANDSHAKE_TURNS_MAX` turns; a read of
  plaintext takes at most `READ_TURNS_MAX` carrier reads. `TlsConnector::connect` and `TlsAcceptor::accept` are the
  blocking drivers' constructors; `CompioTlsStream` is itself a compio stream, so `Connection<CompioTlsStream<Carrier>>`
  carries the protocol inside TLS over either carrier. Both drivers allocate their record buffers once per connection.
  A compio read fills the caller's buffer from its start, as compio's own readers do.
- **The acceptor.** TLS 1.3 only (`with_protocol_versions(&[&TLS13])`), ring as the provider, no client certificate;
  the chain and key are read once; a block that does not decode counts as no certificate; rustls's
  `InconsistentKeys::KeyMismatch` is `KeyMismatch` naming the key's source, anything else rustls refuses is
  `ConfigurationRefused`. A TLS 1.2-only client is refused with rustls's `PeerIncompatible(Tls12NotOfferedOrEnabled)`
  at the server and the `protocol_version` alert at the client.
- **Telemetry.** `ultragraphdb_common_telemetry` uses `TlsStream` through `Read`/`Write` only and `TestPeer` through
  `TestPki::peer`; both keep their behaviour (a stream ending without `close_notify` still reads as `UnexpectedEof`), and
  its Bazel suites pass unchanged.

Bazel after: `protocol_transport` unit 130, compio unit 29 and bdd 18 scenarios, `transport_tls` unit 50 (after the newtypes), bdd 10
scenarios (20 steps), doc test 1; `protocol_scram` and `common_telemetry` (which use the TLS crate and the transport)
pass unchanged. Clippy over the three crates with `--all-targets` reports nothing.

### Unix sockets under Bazel on macOS

A socket path holds at most 103 bytes on macOS (104 bytes of `sun_path`, one the NUL). Every test socket lives inside
`TEST_TMPDIR`. By default that is `<output_base>/execroot/_main/_tmp/<32 hex>`, 129 bytes under
`~/Library/Caches/bazel`. The user chose to shorten the output base, but that alone cannot fit: Bazel resolves
`--output_user_root=/tmp/u` to `/private/tmp/u` and adds a 32-hex output base, so `TEST_TMPDIR` is
14 + 1 + 32 + 21 + 32 = 100 bytes (measured with `bazel info output_base`), leaving 2 bytes for a socket's name. The
root's `.bazelrc` therefore shortens `TEST_TMPDIR`'s own root instead, darwin-scoped:
`test:macos --test_tmpdir=/tmp/ut`, making it `/private/tmp/ut/_tmp/<32 hex>`, 15 + 6 + 32 = 53 bytes, which leaves
103 − 53 − 1 = 49 bytes for a name; the compio tests' longest, `<pid>-<n>-s.sock`, is at most 20. The arithmetic is in
the `.bazelrc` comment and in `tests/utils/socket.rs`, whose `SocketDir` refuses a path past 103 bytes with a message
naming it. The Bazel cache does not move. Linux's 108-byte limit holds under its default root. The `/tmp` link of the
first version is gone; the compio Unix-socket tests pass sandboxed with the sockets in `TEST_TMPDIR`
(`/tmp/ut/_tmp/<hash>/<pid>-<n>-s.sock`). Earlier cargo-mutants runs left a few dangling links `/tmp/ug-<pid>-<n>`;
removing them is the user's call.

## 8.5 Fuzzing

One new decoder, `Address::parse`, configuration text: target `address` in `fuzz/ultragraphdb_protocol_transport`,
seeds in `ultragraphdb_protocol_transport/testdata/fuzz_seeds/address/` (valid TCP by IPv4, IPv6 and name, a Unix path,
and three refusals). A taken address prints as given and parses back to itself, its scheme alone naming its target; a
refusal is an `Address` error naming the text. `cargo +nightly fuzz build --fuzz-dir fuzz/ultragraphdb_protocol_transport`
builds all three targets; the seeds replay (`-runs=0`) for `address`, `framing` and `server_connection`; a 20-second
smoke run of `address` (1.5 million inputs) found nothing. No other decoder was added: the TLS session's records are
rustls's.

## 8.4 End to end

Blocking: a connector against an acceptor's session (`TestPeer`), the acceptor's `accept` against a client session, and
the session pumped over an in-memory `TestPipe` and over non-blocking TCP sockets by one `TestLink::exchange_line`.
Compio: TLS over TCP and over a Unix socket, and a `Connection`'s handshake inside TLS over a Unix socket. A client
verifies the server against a `TlsTrust` holding the test CA in every case (`TestPki::connector`).

## Scenarios

`features/transport_tls/server_tls.feature` holds the four transport-tls scenarios of this change verbatim, bound in
`tests/bdd.rs` beside the six of the earlier change (`transport_tls.feature`); cucumber runs with `fail_on_skipped`.

## Phase 5: mutation testing

`scripts/mutants.sh <crate> <file> --diff cabae5ec` once per changed logic file (30 files, run 1), one fix pass, and a
re-run of the killed mutants on two files (run 2); no file used its third run. The fix pass added
`test_the_bounds_are_a_records_ciphertext_and_the_turns_named` (the `RECORD_SIZE_MAX` arithmetic, 5 mutants) and a
`wants_read` check after `close_notify` (1); the re-run caught all 7. Timeouts count as caught (a no-op `step`,
`transmit`, `write_records` or `shutdown` makes a conversation wait forever). Many mutants were unviable because
cargo-mutants' scratch build took `protocol_wire` mid-change by the wire agent; those files (`tls_connector`,
`listener`, `carrier/mod.rs`, the error files) are covered by tests that construct and call every function.

Survivors, for task 12.1:

| File | Mutant | Why it survives |
|---|---|---|
| `protocol_transport` `server_connection/mod.rs:384` | guard `RequestKind::Commit if open` → `false` | Equivalent: the arm below opens the stream, `Outbound::open` refuses it as `StreamOpen`, and `open` closes with that error's same text and code |
| `transport_tls` `tls_stream_write.rs:22`, compio `carrier_async_write.rs:21`, `compio_tls_stream_async_write.rs:27` | `flush` → `Ok(())` | Every write already writes its records and flushes the stream, so a flush finds nothing waiting; a TCP or Unix socket's own flush does nothing |
| compio `compio_tls_stream/mod.rs:104` | `<` → `==`, `>`, `<=` in `read_stream`'s reserve | The buffer has its capacity unless a read was dropped at its `.await`; no test cancels a read |
| compio `connection/mod.rs:27` (lift) | `<` → `<=` in `fill`'s reserve | Reserving 0 more is a no-op |
| `transport_tls` `utils_test/test_peer/mod.rs:55` | `\|\|` → `&&` in `receive` | Test utility: an empty write into a peer that has not failed processes nothing either way |
| `transport_tls` `utils_test/test_link.rs:62`, `:85` | the `WouldBlock` guard → `true`; `&&` → `\|\|` in `exchange_line` | Test utility: no carrier in the suite fails otherwise; the server answers once whichever way |

## Open for the user and group 9

- **Where role commands are refused.** `ServerConnection` is the client endpoint's connection, so it refuses them; the
  role endpoint's connection, which serves them, is group 9's.
- **`client_core` does not build** against wire's role `Command` variants (an exhaustive `match` in
  `client_core_receive.rs`), so `//ultragraphdb_client/...` and `bazel build //...` fail; `ultragraphdb_error_protocol`'s
  tests fail to compile too. Both are in the wire agent's area and in flux; transport left them alone and asked in the
  coordination file.
- **`TlsAcceptor::from_pem_files(&CertificateChainFile, &PrivateKeyFile)`**: the two paths are newtypes
  (`certificate_chain_file.rs`, `private_key_file.rs`, each `new(path)` and `path()`), so they cannot be swapped, as the
  user decided. The acceptor's logic is unchanged, so no mutants run followed.
