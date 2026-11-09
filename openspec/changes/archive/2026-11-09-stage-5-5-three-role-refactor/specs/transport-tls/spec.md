## ADDED Requirements

### Requirement: A server accepts TLS 1.3 with its configured certificate
`TlsAcceptor` SHALL be built from a certificate chain and its private key, each read once from a PEM file the caller
names, and SHALL refuse at construction a file it cannot read with `CertificateUnreadable` naming the file, a file
holding no certificate or no key with `CertificateEmpty`, and a key that does not match the chain's first certificate
with `KeyMismatch`. It SHALL accept TLS 1.3 only, from the connection's first byte, with no clear-text upgrade
(`SECURITY.md`, *Encryption*), and SHALL complete the handshake before the caller reads or writes a byte.

#### Scenario: A client trusting the CA completes the handshake
- **WHEN** a client trusting the test CA connects to an acceptor holding the certificate the CA issued for `localhost`
- **THEN** the handshake completes with TLS 1.3 and a line sent comes back from the server

#### Scenario: A TLS 1.2 client is refused
- **WHEN** a client offering TLS 1.2 alone connects to the acceptor
- **THEN** the handshake fails as a protocol-version refusal, and the caller reads no byte

#### Scenario: A mismatched key is refused at construction
- **WHEN** an acceptor is built from the test CA's server certificate and another key
- **THEN** construction fails with `KeyMismatch`

### Requirement: One TLS session drives over any carrier
A TLS connection, client or server, SHALL be a session that takes the bytes read from its carrier and gives the bytes to
write to it, with no I/O of its own, so the blocking `std::net` socket, the compio transport and the tokio client shell
drive the same handshake and records. `TlsConnector` and `TlsAcceptor` over a blocking socket SHALL be drivers of that
session.

#### Scenario: The same exchange over two carriers
- **WHEN** a client and a server complete a handshake and exchange one line through the session over an in-memory pipe,
  and again over a TCP socket
- **THEN** both exchanges succeed, and the session's code is the same in both
