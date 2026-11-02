## Why

OTLP span export runs over plain HTTP to a collector beside the process, and the wire protocol's clients connect over
TLS once a server is configured for it (`PROTOCOL.md` §12). Both need one way to verify a server: the certificates
trusted, read once, and a socket wrapped in a connection verified against them. Written once, the trust handling
cannot drift between the two.

## What Changes

- **New crate `ultragraphdb_transport/ultragraphdb_transport_tls`:** `TlsTrust` (the image's CA bundle or the file
  `SSL_CERT_FILE` names, or any PEM source, each anchor once, an empty source refused), `TlsConnector` (TLS 1.3 and
  1.2 over a blocking `std::net` socket, the handshake completed before anything is sent), `TlsError`; behind
  `test-utils`, `TestPki`, a CA and a server certificate rcgen generates per run.
- **Dependencies:** `rustls` 0.23 with ring as its one crypto provider (ring compiles C and assembly through `cc`),
  `rustls-pki-types`; `rcgen` for `test-utils` only.
- **Telemetry:** the OTLP/HTTP client sends to `https://` endpoints through the connector, trusting the image's
  bundle.
- The crate starts with the Ultra lint set (`openspec/protocols/ULTRA_STYLE.md`) and follows the five phases of
  `openspec/protocols/tdd-protocol.md`, BDD scenarios and mutation testing included.

## Impact

- A new transport group folder, `ultragraphdb_transport/`, in the workspace's members.
- `clippy.toml` gains `too-many-lines-threshold = 70` and the in-test allowances the Ultra lint set needs.
