# transport-tls Specification

## Purpose
TLS for the repository's connections: a client verifying a server against certificates read once from the image's CA
bundle or a named PEM source before anything is sent, a server accepting TLS 1.3 alone with its configured certificate,
and one TLS session with no I/O of its own driven over any carrier, so every client and server here establishes trust
the same way.

## Requirements
### Requirement: Trust is read once, from the image's bundle or a named PEM source
`TlsTrust` SHALL read the certificates a connection is verified against from a PEM source: the file `SSL_CERT_FILE`
names, or `/etc/ssl/certs/ca-certificates.crt` when it is unset or empty, or a file or bytes the caller names. It
SHALL keep each anchor once, skip certificates no verifier takes, and refuse a source with no usable certificate with
`TrustStoreEmpty`, and a file it cannot read with `TrustStoreUnreadable` naming the file.

#### Scenario: The bundle SSL_CERT_FILE names is the system bundle
- **WHEN** `SSL_CERT_FILE` names a bundle that holds a test CA
- **THEN** the system trust store holds that one certificate and names that file as its source

#### Scenario: A certificate listed twice is trusted once
- **WHEN** a bundle lists one CA, another CA, and the first again
- **THEN** the trust store holds two certificates

#### Scenario: A source without a usable certificate is refused
- **WHEN** a bundle holds only a block that is no certificate
- **THEN** reading it is refused as an empty trust store

### Requirement: A connection is verified before anything is sent
`TlsConnector` SHALL wrap an open socket in a TLS 1.3 or 1.2 connection for the host the caller names, a DNS name or
an IP address, and SHALL complete the handshake before returning, so a certificate that is untrusted, expired or
issued for another name fails the call with `HandshakeFailed` before any byte of the caller's is sent. A host no
certificate can name SHALL be refused with `InvalidServerName` before the handshake.

#### Scenario: A certificate a trusted CA issued for the host verifies
- **WHEN** a client trusting the test CA connects to a server presenting the certificate it issued for `localhost`
- **THEN** the handshake verifies and a line sent comes back from the server

#### Scenario: A certificate from an untrusted CA is refused
- **WHEN** a client trusting another CA connects to that server
- **THEN** the connection is refused as a failed handshake naming an unknown issuer

#### Scenario: A certificate issued for another name is refused
- **WHEN** a client trusting the test CA connects to that server for `collector.example`
- **THEN** the connection is refused as a failed handshake naming a certificate not valid for the name

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

