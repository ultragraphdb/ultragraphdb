# ultragraphdb_protocol

Three crates implement UltraGraphDB's binary protocol with the sans-I/O pattern
([how to write sans-I/O protocol implementations](https://sans-io.readthedocs.io/how-to-sans-io.html)): plain
synchronous code turns bytes and events into bytes, events and instructions, and a thin shell does the I/O. Its bytes
define the protocol, so the client and the server share the same crates.

| Crate | Layer | I/O | Depends on |
|---|---|---|---|
| `ultragraphdb_protocol_wire` | the format | none | `ultragraphdb_error_protocol`, `ultragraphdb_core_terms`, `xxhash-rust` |
| `ultragraphdb_protocol_transport` | the connection's logic | none | `ultragraphdb_protocol_wire`, `ultragraphdb_error_protocol` |
| `ultragraphdb_protocol_transport_compio` | the I/O shell for compio | compio | `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `compio` |

Two documents beside them specify what the crates implement: `PROTOCOL.md`, the protocol's bytes and the connection's
rules, and `SECURITY.md`, the security architecture those bytes carry.

## `ultragraphdb_protocol_wire`: the format

The crate holds every byte layout of the protocol: the 256-byte frame header, the commands, the XXH3-64
frame and chunk checksums, the request term table, result batches, the Round frame and the write-set record. Each
command's body comes as an owned type with a builder and as a view that reads a received frame in place. Functions take
byte slices and return views or bytes; none knows a connection.

Without I/O, the crate serves wherever the protocol's bytes appear, not only on the network: the write-ahead log
(`ultragraphdb_transactions_log`) stores each round as a Round frame, and a transaction
(`ultragraphdb_transactions_txn`) writes its write-set record for the commit step (`ultragraphdb_transactions_commit`)
to read. Fuzzers call each decoder as a plain function on a byte slice.

## `ultragraphdb_protocol_transport`: the connection's logic

Pure state machines hold a connection's state:

| Type | Role |
|---|---|
| `Framer` | Assembles frames from a byte stream, pull-style: `want` tells the shell how many bytes the frame's pooled buffer must hold, and `advance` returns the next `FrameStep`. It verifies the 256-byte header before trusting its `size`, then takes the rest of the frame into the same buffer. The shell reads straight into the frame's final buffer, by an owned-buffer read (io_uring) or a `&mut [u8]` read, with no copy |
| `ClientHandshake`, `ServerHandshake`, `HandshakeAnswer` | Run the handshake: the client sends Hello on stream 0; the server answers with a Welcome at a protocol in the Hello's range, or refuses |
| `Outbound` | Schedules a connection's outgoing frames across its streams: credit for Result frames, cancellation, a stream's end |
| `BufferPool`, `PooledBuffer`, `ReceivedFrame` | Pool the buffers that frames are read into; a received frame stays in its buffer until viewed |
| `SimNetwork`, `SimStream`, `NetworkFaults`, `NetworkStats` | Simulate the network: seeded links drop, delay, duplicate and split messages; a shell adapts them to its runtime's reader and writer traits |

Tests drive the machines synchronously, with no runtime and no network.

## `ultragraphdb_protocol_transport_compio`: the I/O shell

`Connection` drives a connection over a compio stream: TCP, or a `CompioSimStream` in the simulator. It reads each
frame as its `Framer` asks, into the pooled buffer the frame stays in, writes frames whole, runs the handshake machines,
and leaves streams to `Outbound`. `CompioSimStream` adapts the simulated network to compio's reader and writer traits.
The shell adds no protocol rule, so its tests check the machines' behaviour over real and simulated streams.

The server runs on compio and uses this shell. Another runtime gets its own shell crate over the same machines; that is
how the Rust client offers tokio (`ultragraphdb_client_tokio`) without compiling compio into client
applications.

## Why the split

- **One format, many users.** The log, the transaction engine and the network all speak the wire's bytes; only the
  network needs I/O.
- **Runtimes are the shell's choice.** The protocol's logic knows neither compio nor tokio: the server runs on compio,
  the Rust client offers tokio, and a new runtime needs only a new shell.
- **Deterministic tests.** Tests run the machines synchronously and the shells over a seeded simulated network, so a
  failure replays exactly.
- **Native clients later.** A client in another language implements the same bytes from the specification, which
  states every layout in its own words, independent of this Rust code. See `openspec/notes/native-clients.md`.
