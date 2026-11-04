# UltraGraphDB security architecture

This file describes how UltraGraphDB authenticates a connection, authorizes each request and protects traffic: the
principals, the authentication mechanisms, role-based access control, the security catalog, auditing and encryption.
`PROTOCOL.md` beside it specifies the bytes these use; "Protocol touchpoints" maps one to the other. The design, its
reasoning and its check against Dgraph's ACL are in `openspec/notes/security.md`.

## Principles

| Principle | What it means here |
|---|---|
| Secure by default | Every connection to a cluster authenticates: a client's against the principals, a role's against the role secret. A test server alone, such as the Rust client's, runs without authentication, on loopback |
| Fail closed | An action, scope or capability the authorizer does not know is denied |
| Complete mediation | Every read and write is checked where the store is read or written, never by rewriting a request |
| Current state | Authorization uses the grants as they stand; a revocation holds from its epoch on |
| Least privilege | Administering the cluster grants no access to a namespace's data |
| Standard mechanisms | SASL, SCRAM, OAuth bearer tokens, OpenID Connect and TLS 1.3; no cryptography of the protocol's own |
| Credentials as verifiers, secrets out of logs | The store keeps a password only as its SCRAM verifier; no password, token or authentication message reaches a log or an error |

## Trust boundaries

| Boundary | Assumption | Consequence |
|---|---|---|
| The client network, with the client endpoint set to `plaintext = true` | Trusted: no one reads or alters frames | Authentication protects the password, not the connection after it. Bearer tokens are accepted only over TLS |
| The client network, with TLS | Untrusted | TLS 1.3 protects every frame; from stage 8, channel binding (`SCRAM-SHA-256-PLUS`) ties SCRAM to the connection |
| The network between roles, clear text until stage 8 brings TLS between roles | Trusted: no one reads or alters frames | The role secret authenticates each role connection, not the frames after it |
| The server's disk and configuration | Trusted | Verifiers, keys and the bootstrap credential are kept there; encryption at rest is outside this architecture |
| An identity provider | Trusted for the identities it asserts, by its public keys | Its groups grant roles only through mappings an administrator writes |
| The server's clock | Synchronised, within a token check's minute of leeway | A clock off by more refuses valid tokens or accepts expired ones |

## Principals

A principal is an identity the server authenticates: a person, or a service.

| Field | Meaning |
|---|---|
| `id` | u64, unique and rising, assigned by the metadata group when it creates the principal. 0 names no principal: a role connection's, which acts for none, or a test server's anonymous principal |
| `name` | UTF-8, unique in the cluster, prepared with SASLprep |
| credentials | a SCRAM-SHA-256 verifier: salt, iteration count, `StoredKey`, `ServerKey` |
| external identities | each an (issuer, subject) pair an identity provider asserts, unique in the cluster |
| `enabled` | a disabled principal authenticates no more |
| roles | the roles granted to it |

Principals are cluster-wide and live in the metadata group, which every frontend authenticates against. A tenant is a
namespace, and a tenant's principals hold grants in their namespace only.

The **bootstrap administrator** is the metadata group's first principal, created at the cluster's first start from a
password the operator supplies in a secret file the configuration names, and holds `cluster_admin`. The frontend reads
the file once, when it creates the cluster, and the metadata group keeps only the verifier derived from it; the
operator then removes the file. The cluster has no default credential: without the secret file it creates no
principal, and a cluster with no principal refuses every connection. Removing the last principal holding
`cluster_admin` is refused.

## Authentication

### The exchange

A connection authenticates by a SASL exchange (RFC 4422) on stream 0, after Welcome and before its first request. The
Welcome says that authentication is required, as the frontend's client endpoint always does, and lists the mechanisms
the server accepts. The client sends Authenticate naming one, and the two sides exchange the mechanism's messages until
the server's final Authenticate names the principal and when its credential expires, or the server refuses with code
19 and closes the connection. A request before that is refused with code 20 on its own stream, and the connection
stays open for authentication until its timeout. Every later request acts for the principal.

### Mechanisms

| Mechanism | Credential | Available |
|---|---|---|
| `SCRAM-SHA-256` (RFC 5802, RFC 7677) | a password, which never crosses the connection; the server proves it holds the verifier | always, in clear text and inside TLS |
| `SCRAM-SHA-256-PLUS` | as SCRAM, bound to the TLS channel (`tls-exporter`, RFC 9266) | with TLS, from stage 8 |
| `OAUTHBEARER` (RFC 7628) | a JSON Web Token from a trusted issuer: an OpenID Connect token for single sign-on, or a service's token | with TLS (RFC 6750 §5.3), from stage 8 |
| `EXTERNAL` | the TLS client certificate | with TLS, for services and replicas, from stage 8 |

A further mechanism, Kerberos's `GSSAPI` among them, is a new SASL name and needs no change to the protocol.

### Passwords

- A password is at least 15 characters (NIST SP 800-63B-4) and may be at least 64; it has no composition rules; one on
  the operator's list of common or breached passwords is refused. It is prepared with SASLprep before its key is
  derived, as RFC 5802 requires, so a client in any language derives the same key from the same password.
- The server stores SCRAM-SHA-256's verifier, PBKDF2-HMAC-SHA256 at 600,000 iterations by default (the OWASP Password
  Storage Cheat Sheet), from 4,096 (RFC 7677) to 10,000,000. The client runs the iterations, and may keep the derived
  keys for a password and salt it has seen, so the count costs the server nothing at login. A client refuses a count
  outside that range: below it, as RFC 7677 requires; above it, because the server names the count and a count no
  password needs, up to 2^32 − 1, would hold the client for hours. The ceiling is 16 times the default, room for the
  OWASP figure to rise, and seconds of work at most.
- Changing a password, disabling a principal or removing a mechanism ends the principal's open connections at once,
  with code 22.

### Tokens and single sign-on

A **trusted issuer** is an entry of the security catalog:

| Field | Meaning |
|---|---|
| issuer | the `iss` it signs as, and its OpenID Connect discovery document |
| audience | the `aud` its tokens for this cluster carry |
| algorithm | the one signature algorithm accepted from it: RS256, ES256 or EdDSA |
| keys | its public keys by key id: configured, or read from its `jwks_uri` over TLS and refreshed when an unknown key id arrives |
| subject claim | the claim naming a person stably: `sub` by default; for Microsoft Entra ID, `oid` |
| group claim, group mappings | the claim listing a person's groups, and per group the roles it grants |

A token is accepted when, as RFC 8725 sets out: `iss` is a trusted issuer; its algorithm is the one the issuer's entry
pins, never read from the token; its key is the issuer's key of its key id; its type is the one the issuer sets, where
it sets one; `aud` is the entry's audience; `exp` is present and not past; `nbf` and `iat` lie within a minute's
leeway; and it lives at most an hour by default.

A token names an external identity, the issuer and its subject claim, and the principal holding that identity is the
connection's. An identity no principal holds is refused until an administrator creates its principal. The token's
groups grant roles only through the issuer's mappings, so no issuer confers a role on its own say. A client obtains its
token from the provider, by its own login or the device authorization grant (RFC 8628); on a refusal, the server's
`OAUTHBEARER` reply names the issuer's discovery document and the scope to ask for.

The server issues no tokens: a connection renews its credential on the open connection, and a reconnecting client
authenticates again. The server hands out no token a client could leak, and keeps none it would have to revoke.

### Re-authentication

A credential that expires, as a token does, is renewed on the open connection. Before the expiry the server's final
Authenticate named, the client runs a new exchange on stream 0 while its other streams go on; the exchange must end as
the same principal, and its expiry replaces the old. A credential that expires before a new exchange ends closes the
connection with code 22: its open transactions abort, and its sessions carry over to the principal's next connection.

### Failures

- A failed authentication says nothing of what failed, and takes as long for an unknown principal as for a known one:
  the server answers an unknown name with a verifier derived from a server secret.
- Failed attempts are throttled per principal and per source address, with a growing delay. After 100 consecutive
  failures (NIST SP 800-63B-4), a principal's authentications are refused from every source for a configured interval,
  15 minutes by default, and a success resets the count. The interval ends on its own, so an attacker cannot lock an
  administrator out for good.
- A connection that has not completed its authentication within a configured time, 60 seconds by default as
  PostgreSQL's `authentication_timeout`, is refused with code 20 and closed. Before it authenticates, a connection
  holds no stream and no transaction.
- No password, token, SCRAM message or proof is written to a log or into an error message.

### The config service's keys

The config service holds every role's settings, which name secret files and never hold a secret's value. It reads two keys at start,
each at least 32 bytes, from its own file, and drawn at random by whoever provisions it. The read key reads settings, and every role holds it,
so one read key reads every target of its service: a deployment wanting production apart from staging runs a service per environment.
The admin key also changes settings, and only an administrator holds it. A key changes by a restart of the service.

A role's admin listener, at `[admin] address`, restarts it at a revision on the admin key. A role holds only that key's SHA-256 digest in
`[admin] key_sha256`, so no role can present the key, and a compromised role cannot change any setting or load any other role's revision.
Each listener takes TLS or `plaintext = true` by the endpoint's rule, and compares keys in constant time.

### Running without authentication

No role of a cluster runs without authentication: the frontend's client endpoint authenticates every connection
against the principals, and the development profile does too. A test server, such as the Rust client's, and the
simulator run without it: such a server says so in its Welcome (`auth` 0), listens on the loopback interface only and
refuses to start on any other address, logs that it runs without authentication at every start, and acts for the
anonymous principal, which holds every privilege.

### Role connections

The frontend, compute and storage authenticate each other with the cluster's **role secret**, which every role reads at
start from the file its `[cluster] role_secret_file` setting names, a Kubernetes Secret on a cluster; a role whose file
is missing, unreadable or empty refuses to start. A role connection runs `SCRAM-SHA-256` with the secret as its
password before its first role command, and the accepting role verifies its peer locally, against a verifier it
derives from the secret at start: the principals live in the frontend, which is itself a peer. The secret
authenticates role connections alone; the client endpoint authenticates against the principals only, so the secret
opens no client session. A role connection acts for no principal, and each role command acting for a client names the
client's principal. Rotating the secret takes a rolling restart. Role connections speak clear text in stage 5.5, and
TLS between roles, with certificates per role, arrives with stage 8. In the development container each role's Unix
socket belongs to user 65532 with mode 0600. `PROTOCOL.md` §11.12 gives the exchange's bytes.

## Authorization

### The model

Authorization is hierarchical role-based access control (ANSI INCITS 359). Principals hold roles; roles hold privileges
and other roles; a principal's privileges are the union over every role it holds, directly or through another role. A
role cycle is refused when it would be created. Privileges only allow: an action is denied unless a privilege allows
it, and no rule denies what another allows.

A privilege is an action on a scope. The action's number is the `detail` of a refusal, code 21.

| Action | Number | Allows |
|---|---|---|
| `read` | 1 | reading statements and their properties, and seeing declarations in Describe |
| `write` | 2 | Write, Operation and Commit of statements |
| `declare` | 3 | declarations: predicates, entity types, relations, indexes |
| `drop` | 4 | the Drop command |
| `admin` | 5 | at a namespace: its roles, their grants to principals, and the group mappings onto its roles; at the cluster: also principals and trusted issuers |
| `operate` | 6 | cluster operations: creating and removing namespaces, backup, restore, export |

| Scope | Covers |
|---|---|
| cluster | every namespace, and the cluster's own administration |
| namespace | one namespace: every graph and predicate in it |
| graph | one named graph of a namespace |
| predicate | one predicate of a namespace, in every graph |

A privilege at a scope covers every scope inside it.

| Built-in role | Privileges |
|---|---|
| `cluster_admin` | `admin` and `operate` at cluster scope, and no data of any namespace unless also granted it |
| `owner`, per namespace | every action in the namespace |
| `writer`, per namespace | `read` and `write` |
| `reader`, per namespace | `read` |

### Enforcement

When a transaction begins, the principal's privileges in its namespace resolve into an **access** value: the predicates
and graphs it may read and write, with the roles its token's groups map to. The worker consults it at each read and
write of the store. A request's namespace authorizes nothing; the grants do.

| Request | Check |
|---|---|
| Begin, Prepare | any privilege in the namespace |
| Read, Execute | a statement the principal may not read is absent: a pattern naming a predicate or graph it may not read matches nothing, and one leaving it unbound returns no row of it. Refusing would tell the principal the data exists |
| Describe | only the declarations the principal may read; the schema id hashes what the principal sees |
| Write, Operation | `write` on each predicate and graph written; refused with code 21, ending the transaction |
| Write's declarations | `declare` |
| Commit | the transaction's writes, already checked; the security epoch (below) |
| Drop | `drop` on the scope dropped |
| Register | an authenticated principal; the session belongs to it |
| Commit through a session | the session's principal |
| Administration | `admin` on the scope changed, and holding what it grants: no principal grants more than it holds |
| Cluster operations | `operate` |

A unique predicate's commit-time operation, insert-if-absent or upsert-and-link, reports whether a value is held,
including by a statement the principal may not read; it needs `write` on the predicate, which an administrator grants
knowing that.

### The security catalog and revocation

Principals, their credentials and external identities, roles, grants, trusted issuers and their group mappings form the
security catalog. It lives in the metadata group, outside every namespace, so no namespace drop removes it. It changes
only through the metadata group's commands, so each change is logged, replayed on recovery and replicated as the
group's every command is.

A security change is ordered by the metadata group's log, not by any partition's commit versions, and takes effect at
an epoch, as a catalog version does: the group commits it as pending, and at its next close `c + 1` the sequencer
assigns it the epoch `c + 2`, which that close announces to every partition (`PROTOCOL.md` §11.12). That epoch is the
change's **security epoch**. Every read-write transaction is checked at its commit against the security epoch in effect
at its commit epoch, so a change aborts every transaction that began before it and commits at or after its epoch: from
that epoch on, nothing commits under a revoked privilege. A change to a group mapping applies at the next Begin; a group
an identity provider removes applies at the connection's next re-authentication, within its token's lifetime. A
read-only transaction, which commits nothing, keeps the access its Begin resolved until it ends; a revocation reaches
its next Begin.

## Sessions, statements and the log

- A session belongs to the principal that registered it: its identity is 128 random bits the frontend draws, which the
  metadata group records with the principal. Only that principal commits through it or reads its stored reply, which
  the frontend checks against the metadata group's registry before it routes a commit, so a guessed session identity
  gives neither.
- A prepared statement belongs to its connection, and so to the connection's principal.
- A partition's write-set record names the principal of each committed transaction in `principal`; the metadata
  group's command of a security change names the administrator who made it. Replay does not authorize again: a record
  is in a log because it was authorized when it was decided.

## Audit

Authentication events, authorization refusals and security changes are audit events: structured records with the
principal, its issuer and subject when a token named it, the source address, the action or mechanism, and the outcome,
never a credential. The server writes them to an audit log of their own, apart from its diagnostic log. Security
changes are also in the metadata group's log, as its every command is.

## Encryption

- **TLS 1.3**, through rustls, protects client connections from stage 5.5, and the traffic between roles and replicas
  from stage 8; until then role connections speak clear text, authenticated by the role secret.
- The frontend's client endpoint speaks TLS 1.3 when its `[endpoint]` table names `tls_cert_file` and `tls_key_file`,
  and clear text only when it sets `plaintext = true`; naming neither, or a certificate and `plaintext = true`, stops
  the start with exit code 78, so clear text is always a visible choice. The `dev` profile sets `plaintext = true`. A
  client configured with a trust source verifies the server's certificate for the host it dials before the protocol's
  handshake begins.
- A server configured for TLS speaks only TLS on its port, from the first byte; no clear-text connection upgrades to
  it, so no upgrade can be stripped.
- Certificates authenticate the server to a client; from stage 8, also a client to the server with `EXTERNAL`, and
  roles and replicas to each other.
- XXH3 checksums stay inside TLS: they detect corruption between a frame's writer and its reader, and the log checks
  them at rest.
- TLS seals each record with a sequence number of its own; the frame header's `sequence` numbers frames for the
  protocol's own order and loss checks.

## Protocol touchpoints

| Concern | `PROTOCOL.md` |
|---|---|
| Whether a connection authenticates, and the mechanisms | §5.2, Welcome: `auth`, sections 1 and 2 |
| The exchange and its messages per mechanism | §5.3, Authenticate |
| Authentication, re-authentication and their refusals; a request before authentication | §11.1 |
| TLS 1.3 on the client endpoint, from the first byte | §11.1 |
| Role connections, the role secret, the endpoints and the commands each serves | §2.5, §3, §11.12, §13 |
| Verifying a principal's proof in the metadata group, from stage 8 | §13.8, §13.9 |
| Error codes 19 AuthenticationFailed, 20 Unauthenticated, 21 PermissionDenied with its action, 22 CredentialExpired | §9.4 |
| A session's owner, its random identity and the owner check | §6.1, §11.5 |
| The Drop command and its privilege | §6.5, §11.13 |
| The authentication timeout and the connection bound | §11.1, §11.9 |
| Absent reads and refusals | §11.10 |
| Statements per connection, schema ids per principal | §8.1, §8.3, §11.7 |
| `principal` in the write-set record, and in role commands acting for a client | §12.2, §13.10, §13.12, §13.13 |
| Epochs, from which catalog versions and security changes take effect | §11.12, §13.4 |
| The frame sequence | §2.1, §2.5, §11.2 |

## Where it is built

| Stage | What |
|---|---|
| Stage 5, the protocol block | The bytes above in the wire, the transport and the Rust client: Authenticate and re-authentication, the error codes, `principal` in the write-set record and the session entry, a session's owner; `SCRAM-SHA-256` in the client and the client's test server |
| Stage 5.5, the frontend | The client endpoint: TLS 1.3 from the first byte with the server's certificate and key, or clear text with `plaintext = true`, and the client shells verifying the server's certificate; inside it, the exchange with `SCRAM-SHA-256` against the principals the metadata group holds, required of every connection, a request before it refused while the connection stays open; the password rules, failures answered alike and throttled, the authentication timeout, the connection bound and the audit of authentication; principals and the bootstrap administrator in the metadata group, which a cluster without a principal answers by refusing every connection; sessions registered there with their principal and checked before routing, evicted by the registry alone; the authorizer, holding every privilege for an authenticated principal and failing closed on what it does not know |
| Stage 5.5, between roles | The role secret: `SCRAM-SHA-256` against it on every role connection, each role verifying its peer locally, refused on the client endpoint as a wrong password is; role connections in clear text; Unix sockets of user 65532 with mode 0600 in the development container |
| Stage 7 | The rest of the security catalog, in the metadata group: external identities, roles, grants, trusted issuers and group mappings, the security epoch; enforcement through the access value; Describe filtered; the administration commands and their audit |
| Stage 8 | TLS between roles, with certificates per role; `SCRAM-SHA-256-PLUS`; `OAUTHBEARER` and single sign-on; `EXTERNAL`; replica certificates; Challenge and Verify between frontends and the metadata group's host |

## Standards

RFC 4422 (SASL), RFC 5802 and RFC 7677 (SCRAM), RFC 9266 (TLS 1.3 channel bindings), RFC 7628 (SASL mechanisms for
OAuth), RFC 7519 (JWT), RFC 8725 (JWT best current practices), RFC 6750 (bearer tokens), RFC 8628 (device
authorization grant), RFC 9700 (OAuth 2.0 security best current practice), RFC 8446 (TLS 1.3), OpenID Connect Core 1.0
and Discovery 1.0, NIST SP 800-63B-4, the OWASP Password Storage Cheat Sheet, ANSI INCITS 359-2004.
