## ADDED Requirements

### Requirement: Login and Alter need an admin source
`Login` and `Alter` SHALL require a source IP that is loopback or whitelisted and, when the deployment configures one,
the security token, refusing with Dgraph's errors (`unauthorized ip address: <ip>`).

#### Scenario: A login from an unlisted address is refused
- **WHEN** a client outside the whitelist calls `Login`
- **THEN** it is refused with `unauthorized ip address:` and the client's address

### Requirement: Login issues JWTs
With ACL on, `Login` with a user, a password and a namespace SHALL return an access JWT carrying the user, groups,
namespace and expiry, and a refresh JWT carrying the user, namespace and expiry; a refresh token SHALL return a new pair;
bad credentials SHALL be refused with `invalid username or password`.

#### Scenario: A bad password is refused
- **WHEN** groot logs in with a wrong password
- **THEN** the login fails with `invalid username or password`

#### Scenario: A refresh token yields new tokens
- **WHEN** a client logs in and then logs in with the refresh token
- **THEN** it receives a new access JWT and refresh JWT

#### Scenario: An expired access token is refused
- **WHEN** a request carries an access JWT past its expiry
- **THEN** it is refused with `UNAUTHENTICATED` and a message containing `Token is expired`

### Requirement: Predicate permissions
Read, write and modify permissions (4, 2, 1) on predicates SHALL be granted to groups. A query SHALL silently omit
predicates the caller cannot read; a mutation on a predicate the caller cannot write SHALL be refused with
`PERMISSION_DENIED` naming it; an `Alter` on a predicate SHALL need modify.

#### Scenario: An unreadable predicate is omitted
- **WHEN** a user without read on `salary` queries `name` and `salary`
- **THEN** the result holds `name` and no `salary`, and the query does not fail

#### Scenario: An unwritable predicate is refused
- **WHEN** a user without write on `salary` sets it
- **THEN** the request is refused with `PERMISSION_DENIED` naming `salary`

### Requirement: Guardians and the superadmin
Guardians SHALL bypass predicate permissions in their namespace except on the ACL predicates, and deleting reserved
properties of the groot user or the guardians group SHALL be refused with `Reserved Properties of guardians group and
groot user cannot be deleted.`; the guardians of namespace 0 SHALL be the superadmin, who alone creates, drops and
lists namespaces and allocates IDs, and who may act across namespaces with the galaxy-operation metadata.

#### Scenario: Deleting groot is refused
- **WHEN** ACL is on and a guardian deletes groot's `dgraph.xid`
- **THEN** the request is refused with `Reserved Properties of guardians group and groot user cannot be deleted.`

#### Scenario: A guardian of another namespace is not the superadmin
- **WHEN** a guardian of namespace 2 calls `CreateNamespace`
- **THEN** it is refused with `PERMISSION_DENIED`

### Requirement: Capabilities and postures as Dgraph v25.5.0 checks them
Capabilities SHALL be checked as Dgraph v25.5.0 checks them (cluster administration, tenant administration, assuming a
tenant, leasing uids), with ACL on and off and under each `--security` anonymous posture. With ACL off, every request
SHALL run in namespace 0, and the namespace calls and `drop_all` SHALL need the admin source and token.

#### Scenario: Without ACL every request runs in namespace 0
- **WHEN** ACL is off and a request names namespace 3 in its metadata
- **THEN** it reads and writes namespace 0

### Requirement: Users and groups are managed through DQL
Users, groups, memberships and rules SHALL be stored as the nodes, types and predicates Dgraph uses
(`dgraph.type.User`, `dgraph.type.Group`, `dgraph.type.Rule`, `dgraph.xid`, `dgraph.password`, `dgraph.user.group`,
`dgraph.acl.rule`, `dgraph.rule.predicate`, `dgraph.rule.permission`) and managed through one-request DQL mutations and
the `ultragraph acl` subcommand.

#### Scenario: A group rule takes effect
- **WHEN** a guardian adds a group with read on `name`, adds a user to it, and the user queries `name`
- **THEN** the query returns `name`

### Requirement: The namespace comes from the token
With ACL on, a request's namespace SHALL be the namespace in its access JWT; a namespace a client sends in metadata SHALL
be ignored.

#### Scenario: Client namespace metadata is ignored
- **WHEN** a user of namespace 1 sends namespace 2 in request metadata and queries
- **THEN** the query reads namespace 1
