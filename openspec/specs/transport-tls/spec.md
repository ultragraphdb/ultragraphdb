# transport-tls Specification

## Purpose
The one way a client verifies a server: the certificates trusted, read once from the image's CA bundle or a named PEM
source, and a socket wrapped in a TLS 1.3 or 1.2 connection verified against them before anything is sent, so OTLP
export and the wire protocol's clients cannot drift in how they trust a server.
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
