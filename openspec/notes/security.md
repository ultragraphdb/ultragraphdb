# Security: authentication, authorization and encryption

Status: designed 2026-10-30, for point 53 of `ultragraphdb_protocol/PROTOCOL.md` §13 (the user's decision: design
authentication, authorization and encryption now, RBAC supported from the ground up and added later, TLS and other
advanced encryption implemented later). The design was then checked against Dgraph's ACL and against current practice;
where the two differ it follows the practice, since the port has already found an unusually large number of severe bugs
in Dgraph's Go code (the user's direction, 2026-10-30). "Dgraph's ACL against current practice" holds that check. The
wire changes enter the protocol block; "Phases" says what each later stage builds. The user reviewed the design on
2026-10-30: the phases, absent reads, allow-only privileges, one cluster-wide epoch and the frame sequence are decided,
and the design was readied for single sign-on (SSO) the same day, so SSO can be added at any later stage without a
change to the protocol or the security catalog ("Single sign-on"). A decision still marked *proposed* awaits review.

## What it gives

- Every connection is authenticated before its first request, by a mechanism the client and server both speak, and
  acts for one **principal** from then on. Authentication is on by default.
- Every request is authorized against that principal's privileges in the request's namespace, by role-based access
  control (RBAC): principals hold roles, roles hold privileges, a role may hold other roles. A check happens where the
  data is read or written, against the current grants.
- A session, its stored replies and its prepared statements belong to the principal that made them.
- The log names the principal of every committed record, and authentication events and security changes are audited.
- Single sign-on adds without a protocol change: an OpenID Connect token presented over `OAUTHBEARER`, and
  re-authentication on the open connection before the token expires.
- Traffic is encrypted with TLS 1.3 once it is implemented; until then the protocol runs in clear text on trusted
  networks, as the stage 5 design states (`zero-copy-binary-protocol.md`, "Encryption").

## Assumptions

| Assumption | Consequence |
|---|---|
| Until TLS, the network between clients and the server is trusted: no one reads or alters frames | Authentication protects the password, not the connection after it: a party that can read frames can hijack an authenticated connection. Bearer tokens wait for TLS (see "Mechanisms") |
| The server's disk and its configuration are trusted | Credentials are stored as verifiers, never as passwords; encryption at rest is not part of this design |
| Security changes are rare beside data writes | A security change may abort every transaction in flight (see "Revocation") |
| scrith runs one namespace per tenant and confines its PERSONAL predicates to its gateway (`UltraGraphDB Dual Query Engine Plan.md`, the requirements table) | Privileges reach down to a predicate in a namespace |

## Principals

A principal is an identity the server authenticates: a person, or a service such as scrith's gateway.

| Field | Meaning |
|---|---|
| `id` | u64, numbered as a session is: the commit version of the round that created it, unique and rising. 0 is the anonymous principal of a server running without authentication |
| `name` | UTF-8, unique in the cluster, prepared with SASLprep as SCRAM requires |
| credentials | per mechanism: a SCRAM-SHA-256 verifier (salt, iteration count, `StoredKey`, `ServerKey`) |
| external identities | the identities an identity provider asserts for it, each an (issuer, subject) pair, unique in the cluster: a subject is unique only within its issuer |
| `enabled` | a disabled principal authenticates no more |
| roles | the roles granted to it |

Principals are cluster-wide; what a principal may do in a namespace is its grants there. One cluster serves many
tenants, each a namespace, and a tenant's principals hold grants in their namespace only.

**The bootstrap administrator**, the cluster's first principal, is created with the cluster from a credential the
operator supplies in a secret file the configuration names. The server has no default password: a server requiring
authentication and given no bootstrap credential refuses to create the cluster (CWE-1392). Dgraph instead creates
`groot` with the password `password` in every namespace (`edgraph/access.go:452`, `edgraph/namespace.go:29`).

### Passwords

- At least 15 characters for a password that is the only factor, as NIST SP 800-63B-4 requires, and up to at least
  64; no composition rules; a password on the operator's list of common or breached passwords is refused. Dgraph
  requires 6 (`types/password.go:14`).
- Stored as SCRAM-SHA-256's verifier, PBKDF2-HMAC-SHA256 over the password, at 600,000 iterations by default, as the
  OWASP Password Storage Cheat Sheet gives for PBKDF2-HMAC-SHA256; never below the 4,096 RFC 7677 requires. SCRAM
  runs the iterations on the client, which may keep the derived keys for a password and salt it has seen (RFC 5802), so
  the count costs the server nothing at login. PostgreSQL defaults to 4,096; this design follows OWASP.
- Changing a password, disabling a principal or removing a mechanism ends its open connections at their next request.

## Authentication

### The exchange

Authentication is a SASL exchange (RFC 4422) on stream 0, between Welcome and the first request, as PostgreSQL's and
MongoDB's are: the server lists its mechanisms, the client picks one, and they exchange the mechanism's opaque
messages until the server accepts or refuses. A new mechanism is a new name, not a new command.

1. Hello, as now, plus the client's `max_frame` (point 44).
2. Welcome, as now, plus: whether authentication is required, and the mechanisms the server accepts, by name.
3. When required: the client sends **Authenticate** (a new command, both directions, stream 0) naming the mechanism
   and carrying its first message. The server answers Authenticate with the mechanism's next message, and the client
   answers in turn, until the server's last Authenticate, flagged final, carries the principal's id and the
   credential's expiry, 0 for none. A refusal is an Error on stream 0, after which the server closes the connection.
4. A request before the exchange completes is refused with a code of its own, and the connection closes.

**Re-authentication.** A connection's credential may expire: a token at its `exp`, and with TLS a client certificate at
its end of validity; a SCRAM credential has none. Before it does, the client authenticates again on the open
connection: a new Authenticate exchange on stream 0, by any mechanism the server lists, while requests on the other
streams go on. The exchange must end as the same principal; another principal is refused, and the connection closes.
The new credential's expiry replaces the old. A connection whose credential expires without a new exchange is sent an
Error of a code of its own on stream 0 and closed: its open transactions abort, and its sessions, which belong to the
principal, carry over to the next connection. The Rust client authenticates again ahead of the expiry. Kafka's
re-authentication (KIP-368) and Neo4j's Bolt `LOGOFF` and `LOGON` keep a connection across a credential's renewal the
same way.

**Authentication is required by default.** Running without it is an explicit option of the server's configuration,
for tests, the simulator and development: the server then listens on the loopback interface only, logs that it runs
without authentication at every start, and acts for the anonymous principal, which holds every privilege. MongoDB's
binding to localhost and Redis's protected mode take the same line. Dgraph runs with no access control unless a secret
file is configured, and with it off every check passes (`edgraph/access.go:969`, `edgraph/predicate_acl.go:218`;
CWE-1188).

### Mechanisms

| Mechanism | What crosses | Stored on the server | When |
|---|---|---|---|
| `SCRAM-SHA-256` (RFC 5802, RFC 7677) | a salted challenge-response: two round trips; the password never crosses, and the server proves it holds the verifier | salt, iteration count, `StoredKey`, `ServerKey` | the first mechanism built |
| `OAUTHBEARER` (RFC 7628) | a JSON Web Token (RFC 7519) from an identity provider the cluster trusts: an OpenID Connect token for single sign-on, or a service's token. On a refusal the server's reply names the issuer's discovery document and the scope to ask for, so a client knows where to get a token | the trusted issuers (see "Single sign-on") | with TLS: a bearer token crosses only an encrypted connection (RFC 6750 §5.3) |
| `SCRAM-SHA-256-PLUS` | SCRAM bound to the TLS channel (`tls-exporter`, RFC 9266), so a hijacker cannot reuse it | as SCRAM | with TLS |
| `EXTERNAL` | nothing: the TLS client certificate names the principal | the certificate subjects that name each principal | with TLS, for replicas and services |

Every primitive these need is in every target language's standard or common libraries (SHA-256, HMAC, PBKDF2, the
signature verifications of RS256, ES256 and EdDSA, JWT), as the protocol's wire choices must be (`native-clients.md`);
in Rust they are RustCrypto's pure-Rust crates, the server verifying signatures and never making one.

A token is checked as RFC 8725 sets out: `iss` one of the trusted issuers; the algorithm the one that issuer's entry
pins, RS256, ES256 or EdDSA as the issuer signs, and never read from the token; the key chosen by its key id among the
issuer's keys, so keys rotate; the explicit type checked where the issuer sets one; `aud` the audience the entry names,
`exp` required, `nbf` and `iat` within a minute's leeway, and a lifetime of at most an hour by default. The token names
an external identity and nothing more: the issuer and the subject claim its entry names. Roles come from the security
catalog; an issuer's groups count only through the mappings an administrator wrote into its entry, so no issuer
confers a role on its own say. The fork's principal records the method that authenticated it for the same reason: "a
policy that reads `Principal.Groups` without checking `Method` would let any issuer that can mint a `guardians` group
membership confer superadmin" (`x/principal.go`).

**The server mints no tokens.** A connection authenticates as its principal and renews its credential on the open
connection; a reconnecting client authenticates again, and its sessions carry over because they belong to the
principal. There is no access token to steal from a client and no refresh token to revoke. Dgraph's login mints both
(see the check below).

### Single sign-on

Single sign-on is an identity provider's OpenID Connect token over `OAUTHBEARER`, as PostgreSQL 18, MongoDB
(`MONGODB-OIDC`) and Kafka take it. The client obtains the token from the provider, by its own login or the device
authorization grant (RFC 8628) a command-line client uses; the server never sees a password. A **trusted issuer** is an
entry of the security catalog, written by an administrator and logged as every security change is:

| Field | Meaning |
|---|---|
| issuer | the `iss` it signs as, and its discovery document (OpenID Connect Discovery) |
| audience | the `aud` its tokens for this cluster carry |
| algorithm | the one signature algorithm accepted from it: RS256, ES256 or EdDSA |
| keys | its public keys by key id: configured, or read from its `jwks_uri` over TLS and refreshed when an unknown key id arrives |
| subject claim | the claim naming a person stably: `sub` by default; Microsoft Entra ID's `oid`, since its `sub` differs per application |
| group claim, group mappings | the claim listing the person's groups, and per group the roles it grants, each scoped as any grant is |
| provisioning | an identity no principal holds is refused until an administrator creates its principal (decided 2026-10-30) |

A connection authenticated by a token keeps the token's groups, and every transaction's access value is resolved from
the principal's grants and the current mappings of those groups, so a mapping changed in the catalog applies at the
next Begin. A group the provider removes applies at the next re-authentication, within the token's lifetime.

### Failures

- A failed authentication says nothing of what failed: no "unknown principal" beside "wrong password". The server
  spends the same work on an unknown principal as on a known one, with a verifier derived from a server secret, so
  timing tells nothing either.
- Failed attempts are throttled per principal and per source address, with a growing delay, and a principal takes at
  most 100 consecutive failures before it waits for an administrator or a configured interval, as NIST SP 800-63B-4
  bounds online guessing. Dgraph's login has no limit (`edgraph/access.go:42-108`).
- No password, token, SCRAM message or proof is written to a log or into an error message (CWE-532). An authentication
  event is audited with the principal's name, its issuer and subject when a token named it, the source address, the
  mechanism and the outcome.

## Authorization

### The model

The model is the hierarchical RBAC of ANSI INCITS 359, the NIST model: principals hold roles, roles hold privileges and
other roles, and a principal's privileges are the union over every role it holds, directly or through another role. A
role cycle is refused when it would be created.

A **privilege** is an action on a scope:

| Action | Allows |
|---|---|
| `read` | reading statements and their properties, and seeing declarations in Describe |
| `write` | Write, Operation and Commit of statements |
| `declare` | declarations: predicates, entity types, relations, indexes |
| `drop` | the Drop command (point 28) |
| `admin` | creating and changing principals, roles and grants within the scope |
| `operate` | cluster operations: creating and removing namespaces, backup, restore, export |

| Scope | Covers |
|---|---|
| cluster | every namespace, and the cluster's own administration |
| namespace | one namespace: every graph and predicate in it |
| graph | one named graph of a namespace |
| predicate | one predicate of a namespace, in every graph |

A privilege at a scope covers every scope inside it. Privileges only allow: an action is denied unless a privilege
allows it, and no rule denies what another allows. PostgreSQL's privileges work this way; a later deny rule would be an
addition, not a change (decided 2026-10-30).

**Built-in roles**, created with the cluster, follow least privilege and separate administration from data (Saltzer and
Schroeder's separation of privilege): `cluster_admin` holds `admin` and `operate` at cluster scope and reads and writes
no namespace's data unless also granted that; per namespace, `owner` holds every action in it, `writer` `read` and
`write`, and `reader` `read`. The bootstrap administrator holds `cluster_admin`. Dgraph's guardians pass every predicate check
in their namespace (`edgraph/access.go:1000`), and the root namespace's guardians act in any tenant
(`CapAssumeTenant`, `edgraph/predicate_acl.go:32`).

### Where requests are checked

Every check happens where data is read or written: complete mediation, in Saltzer and Schroeder's terms. The
principal's privileges in a namespace are resolved when a transaction begins, into an **access** value the transaction
carries, the predicates and graphs it may read and write; the worker consults it at each read and write of the store,
so a check costs a set lookup and no request construct can reach data around it. Dgraph instead rewrites a parsed query,
removing the predicates it finds blocked (`edgraph/access.go:1053`, `removePredsFromQuery`), so a construct its
extraction misses reads unchecked.

A request's namespace is checked against the principal's grants; a namespace a client names authorizes nothing. The
fork found a guardian of any tenant could claim namespace 0 through client metadata and administer the cluster
(`edgraph/predicate_acl.go`, `authSuperAdmin`'s comment).

| Request | Check |
|---|---|
| Begin | `read` somewhere in the namespace |
| Read, Execute | A statement the principal may not read is absent: a pattern naming such a predicate or graph matches nothing, and a pattern leaving it unbound returns no row of it, as Neo4j hides what a principal may not traverse and PostgreSQL's row security hides rows. Refusing would tell the principal the data exists (decided 2026-10-30) |
| Describe | only the declarations the principal may read; the schema id hashes what the principal sees, so a statement is stale for a principal whose view changed |
| Write, Operation | `write` on each predicate and graph written; a refusal is a code of its own on the stream, ending the transaction |
| Write's declarations | `declare` |
| Commit | the checks above, already made; and the revocation check below |
| Drop | `drop` on the dropped scope |
| Register | an authenticated principal |
| Commit through a session | the session's own principal (see "Sessions") |
| Administration | `admin` on the scope changed, and holding what it grants: no principal grants more than it holds |
| Cluster operations | `operate` |

A unique predicate's commit-time operation (insert-if-absent, upsert-and-link) finds a holder the principal may not
read: it tells the principal that the value is held. The check needs `write` on the predicate, which an administrator
grants knowing that.

An unknown action or a scope the checks do not know is denied: the authorizer fails closed, as the fork's
`AuthorizeCapability` does for an unhandled capability.

### Revocation

The security catalog is data in the store, changed only by proposals of the commit step, so it is logged, replayed and
(stage 10) replicated as every write is. Every transaction's read set holds the catalog's **security epoch** key, as it
holds its namespace's catalog range (stage 5 design, "Read sets at statement granularity"); every security change
writes it. A revocation therefore aborts every transaction that began before it and commits after it: from the
revocation's commit version on, nothing commits under the revoked privilege. Security changes are rare, so one
cluster-wide epoch suffices (decided 2026-10-30). Dgraph decides by the groups an access token carried when it was minted, up
to six hours before (`edgraph/access.go:243`, `x/vault.go:46`), and refreshes its ACL cache asynchronously
(`edgraph/access.go:316`).

The catalog lives under the system prefix (`\xffultragraphdb/security/`, beside the session table), outside every
namespace, so no namespace drop removes it.

## Sessions and statements

- A session's entry records the principal that registered it. A Commit through another principal's session is refused
  with the permission code, so a guessed session number gives neither a commit nor a stored reply.
- A retried Commit on a new connection is answered from the table only when the new connection authenticated as the
  session's principal.
- Prepared statements belong to the connection (point 35), and so to its principal.

## The log and the audit

The write-set record's fixed part gains `principal` (u64): the principal of the transaction, the administrator of a
security change, 0 without authentication. The log names who wrote each record without a lookup of the session table,
a session 0 commit included. Replay does not authorize again: a record is in the log because it was authorized when it
was decided.

Authentication events, refusals of authorization and security changes are audit events, written as structured records
with the principal, the source address, the action and the outcome, and never a credential. Security changes are also
in the log, as every write is. Dgraph records access decisions only as annotations of a trace span
(`edgraph/access.go:1019`).

## Encryption

- **TLS 1.3** through rustls, the one cryptographic dependency the port already documents as its exception to pure
  Rust (`dgraph-port-overview.md`), for client connections and, in stage 10, between replicas.
- **From the first byte.** A server configured for TLS speaks only TLS on its port; nothing upgrades a clear-text
  connection, as STARTTLS does, so nothing can strip the upgrade. The ALPN name is `ultragraphdb/1` — *proposed*.
- **Certificates** authenticate the server to the client, and with `EXTERNAL` a client to the server; replicas
  authenticate each other by certificate.
- **XXH3 stays.** Inside TLS it is redundant against tampering but still detects corruption between the frame's
  writer and its reader, and it is what the log checks at rest.
- **The reserved nonce.** The header's 16 bytes were kept for a cipher of the protocol's own
  (`zero-copy-binary-protocol.md`). With TLS there is no such cipher: TLS seals each record with a sequence number of
  its own, and a cipher of the protocol's own is the custom cryptography current practice avoids. The field becomes
  point 32's frame sequence, unique per frame (decided 2026-10-30).

## Wire changes

| Change | Where |
|---|---|
| Hello: the client's `max_frame` | Hello's command header (point 44) |
| Welcome: `auth_required`, and the mechanisms as a string list | Welcome's command header and sections |
| Authenticate, command 18 | a new command, both directions, stream 0, repeatable on an open connection for re-authentication: a step number, a final flag, the mechanism's name in its first message, the mechanism's bytes, the principal's id and the credential's expiry in the server's final message |
| Error codes 19 AuthenticationFailed (the connection closes), 20 Unauthenticated (a request before the exchange; the connection closes), 21 PermissionDenied (`detail` the action; the request refused, its transaction ended), 22 CredentialExpired (no re-authentication before the expiry; the connection closes) | section 9.4, after point 29's 18, NotCommitted |
| The write-set record's fixed part: `principal` | section 12.2: 40 bytes |
| The session entry: the session's principal | the commit crate's `SessionEntry` |

The numbers above are placeholders until the protocol block assigns them.

## Phases

| When | What |
|---|---|
| Protocol block (stage 5) | Every wire change above, in the specification, the wire, the transport's handshake machine and the Rust client: the Authenticate exchange and re-authentication, the codes, `principal` in the record and the session entry, the session's owner checked. `SCRAM-SHA-256` in the client core and the scripted server |
| The server (group 10) | The server's side of the exchange: `SCRAM-SHA-256`, the password rules, throttling, the audit of authentication; running without authentication on loopback only; the bootstrap administrator from its secret file; the authorizer seam, holding every privilege for an authenticated principal and failing closed on what it does not know |
| Stage 7 | The security catalog: principals, their external identities, roles, grants, trusted issuers and their group mappings, the epoch; RBAC enforcement through the access value; Describe filtered; the administration commands and their audit |
| With TLS | TLS 1.3 on the port; `SCRAM-SHA-256-PLUS`; `OAUTHBEARER`, and with it single sign-on; `EXTERNAL`; replica certificates |
| Any time after | A further mechanism by its SASL name, Kerberos's `GSSAPI` among them, needing no change to the protocol |

## Dgraph's ACL against current practice

Checked 2026-10-30 against the Dgraph fork at `/Users/marvin/RustroverProjects/fork/dgraph`. "Follows" means this
design takes Dgraph's choice; every other row is a divergence the design resolves for the practice.

| Practice | Dgraph | This design |
|---|---|---|
| Secure by default, fail closed (Saltzer and Schroeder's fail-safe defaults; CWE-1188) | No access control unless a secret file is configured; with it off every query, guardian and super-admin check passes, and the fork's comments keep two capabilities open by design (`edgraph/access.go:969`, `edgraph/predicate_acl.go:32-50`, `:218`) | Authentication required by default; running without it is explicit and loopback only |
| No default credentials (CWE-1392) | `groot` / `password` in every namespace (`edgraph/access.go:452`, `edgraph/namespace.go:29`) | The bootstrap credential from the operator's secret file; none, no cluster |
| Password length (NIST SP 800-63B-4) | 6 characters (`types/password.go:14`); bcrypt keeps only the first 72 bytes | 15 at least, 64 at most at least; a breached-password list; PBKDF2 has no length cut |
| Password storage (OWASP) | bcrypt, cost 10 (`types/password.go:23`): within OWASP's guidance | Follows the intent: PBKDF2-HMAC-SHA256 at 600,000 iterations, as SCRAM requires PBKDF2 |
| Online guessing bounded (NIST SP 800-63B-4) | No limit on login (`edgraph/access.go:42-108`) | Throttled per principal and source; at most 100 consecutive failures |
| No secrets in logs or errors (CWE-532) | A rejected refresh token is put in the error that login logs (`edgraph/access.go:119`, `:69`); a wrong auth token is echoed in its error (`edgraph/server.go:2269`) | No credential, token or SCRAM message in any log or error |
| Asymmetric token signing, verifiers cannot mint (RFC 8725) | HS256 by default (`x/vault.go:48`): every node verifying a token holds the secret that mints one | The server mints no tokens; an issuer's tokens verified with its public keys, by the algorithm its entry pins |
| Explicitly typed tokens (RFC 8725 §3.11) | Access and refresh tokens share one key and have no type: `validateToken` takes either (`edgraph/access.go:170`, `:243-274`) | No server tokens; an external token's type is checked |
| Audience and issuer checked (RFC 8725 §3.8, §3.9) | Neither claim is set or checked | `iss` a trusted issuer and `aud` its entry's audience; an identity is the (issuer, subject) pair |
| Short-lived, revocable tokens; refresh tokens rotated or bound (RFC 9700 §4.14) | Access tokens live 6 hours and refresh tokens 30 days (`x/vault.go:46-47`); neither is revoked by a password change or a removed user's groups, and refresh tokens do not rotate | No refresh tokens; an external token at most an hour, renewed by re-authentication on the open connection; a connection ends when its credential expires or its principal's credential changes |
| Authorize against current state | Groups are read from the access token, fixed for its life (`edgraph/access.go:243`); the ACL cache refreshes asynchronously (`:316`) | Roles from the catalog; revocation through the epoch, exact at its commit version |
| Complete mediation at the point of access (Saltzer and Schroeder) | A rewrite of the parsed query removes blocked predicates (`edgraph/access.go:1053`) | The access value checked at each store read and write |
| Tenant identity from the credential, never from the client | Fixed in the fork: a tenant's guardian could claim namespace 0 through client metadata (`edgraph/predicate_acl.go`, `authSuperAdmin`) | A namespace authorizes nothing; grants do |
| Least privilege, administration apart from data | Guardians pass every predicate check in their namespace (`edgraph/access.go:1000`); the root namespace's guardians act in any tenant (`edgraph/predicate_acl.go:32`) | `cluster_admin` administers and operates; data needs a grant |
| Durable audit of authentication and authorization (OWASP ASVS) | Trace span annotations only (`edgraph/access.go:1019`) | Structured audit events; security changes in the log |
| Algorithm pinned, never read from the token (RFC 8725 §3.1) | Pinned to the configured algorithm (`x/jwt_helper.go:35`) | Follows |
| Expiry required | Required: a token without `exp` is refused (`edgraph/access.go:177`) | Follows |
| Constant-time secret comparison | `subtle.ConstantTimeCompare` (`edgraph/server.go:2268`); bcrypt's comparison | Follows: SCRAM's proof compared in constant time |
| Uniform login failure | One error for every failure (`x.ErrorInvalidLogin`) | Follows, timing included |

## Precedent

| System | What this design takes |
|---|---|
| Dgraph (the port's source) | Predicate-level privileges (`Read`, `Write`, `Modify`, `acl/utils.go`), namespaces as tenants, the fork's principal with its authentication method (`x/principal.go`) and its fail-closed capability check; what it does not take is in the check above |
| PostgreSQL | The SASL exchange in the startup, SCRAM-SHA-256 with channel binding, `OAUTHBEARER` for single sign-on (PostgreSQL 18), roles holding roles, allow-only privileges, rows a principal may not see hidden |
| MongoDB | SCRAM in the handshake; roles of privileges, each an action on a resource; localhost only until authentication is configured; OpenID Connect with an identity provider's groups mapped to roles |
| Neo4j | Fine-grained graph privileges, and data a principal may not read invisible rather than refused; `LOGOFF` and `LOGON` renewing a connection's credential |
| Kafka | `OAUTHBEARER`, and re-authentication on an open connection (KIP-368) |
| FoundationDB | Authorization by signed tokens verified against trusted public keys, scoped to tenants |
| NIST RBAC (ANSI INCITS 359) | Hierarchical RBAC: the role graph, privileges as the union over it |

## Open questions

Answered 2026-10-30: the phases as the table gives them; a read of data the principal may not read finds it absent;
privileges only allow; one cluster-wide security epoch; the reserved nonce becomes the frame sequence; an identity a
trusted issuer asserts and no principal holds is refused until an administrator creates its principal.

## Sources

- RFC 4422 (SASL), RFC 5802 (SCRAM), RFC 7677 (SCRAM-SHA-256), RFC 9266 (TLS 1.3 channel bindings), RFC 7519 (JWT),
  RFC 8725 (JWT best current practices), RFC 6750 (bearer tokens), RFC 7628 (SASL mechanisms for OAuth), RFC 8628
  (OAuth 2.0 device authorization grant), RFC 9700 (OAuth 2.0 security best current practice), RFC 8446 (TLS 1.3)
- OpenID Connect Core 1.0; OpenID Connect Discovery 1.0
- Apache Kafka KIP-368, "Allow SASL Connections to Periodically Re-Authenticate"
- NIST SP 800-63B-4, Digital Identity Guidelines: Authentication and Authenticator Management
- OWASP Password Storage Cheat Sheet; OWASP Application Security Verification Standard; CWE-532, CWE-1188, CWE-1392
- ANSI INCITS 359-2004, Role Based Access Control
- J. H. Saltzer and M. D. Schroeder, "The Protection of Information in Computer Systems", Proceedings of the IEEE,
  1975: fail-safe defaults, complete mediation, least privilege, separation of privilege
- The Dgraph fork at `/Users/marvin/RustroverProjects/fork/dgraph`: `acl/utils.go`, `edgraph/access.go`,
  `edgraph/predicate_acl.go`, `edgraph/server.go`, `edgraph/namespace.go`, `types/password.go`, `x/jwt_helper.go`,
  `x/principal.go`, `x/vault.go`, `x/x.go`
- `openspec/changes/stage-5-transactions-and-binary-protocol/ zero-copy-binary-protocol.md`, "Encryption" and the
  header; `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`, the requirements table; `dgraph-port-overview.md`
