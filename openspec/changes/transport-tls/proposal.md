## Why

OTLP span export runs over plain HTTP to a collector beside the process, and the wire protocol's clients connect over
TLS once a server is configured for it (`PROTOCOL.md` §12). Both need one way to verify a server: the certificates
trusted, read once, and a socket wrapped in a connection verified against them. Written once, the trust handling
cannot drift between the two.

## What Changes

- **New crate `ultragraphdb_transport/ultragraphdb_transport_tls`:** `TlsTrust` (the image's CA bundle or the file
  `SSL_CERT_FILE` names, or any PEM source, each anchor once, an empty source refused), `TlsConnector` (TLS 1.3 and
  1.2 over any blocking stream, the handshake completed before anything is sent), `TlsError`; behind `test-utils`,
  `TestPki`, a CA and a server certificate rcgen generates per run, served by `TestPeer`, an in-memory rustls server,
  so tests need neither a socket nor a thread.
- **Dependencies:** `rustls` 0.23 with ring as its one crypto provider (ring compiles C and assembly through `cc`),
  `rustls-pki-types`; `rcgen` for `test-utils` only.
- **Telemetry:** the OTLP/HTTP client sends to `https://` endpoints through the connector, trusting the image's
  bundle, over the connections a `Dial` opens; each step waits at most its timeout, with no clock.
- The crate follows the five phases of `openspec/protocols/tdd-protocol.md`, BDD scenarios and mutation testing
  included, under the workspace's one lint table.

## Impact

- A new transport group folder, `ultragraphdb_transport/`, in the workspace's members.
