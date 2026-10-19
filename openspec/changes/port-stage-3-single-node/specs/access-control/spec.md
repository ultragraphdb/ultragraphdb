## ADDED Requirements

### Requirement: Login issues JWTs
`Login` with a user, a password and a namespace SHALL return an access JWT carrying the user, groups and namespace
and a refresh JWT; a refresh token SHALL return a new pair; bad credentials SHALL be refused with Dgraph's
invalid-login error.

#### Scenario: A bad password is refused
- **WHEN** groot logs in with a wrong password
- **THEN** the login fails with Dgraph's invalid-login error

#### Scenario: A refresh token yields new tokens
- **WHEN** a client logs in and then logs in with the refresh token
- **THEN** it receives a new access JWT and refresh JWT

#### Scenario: An expired access token is refused
- **WHEN** a request carries an access JWT past its expiry
- **THEN** it is refused with `UNAUTHENTICATED`

### Requirement: Predicate permissions
Read, write and modify permissions (4, 2, 1) on predicates SHALL be granted to groups. A query SHALL silently omit
predicates the caller cannot read; a mutation on a predicate the caller cannot write SHALL be refused naming it;
an `Alter` on a predicate SHALL need modify.

#### Scenario: An unreadable predicate is omitted
- **WHEN** a user without read on `salary` queries `name` and `salary`
- **THEN** the result holds `name` and no `salary`, and the query does not fail

#### Scenario: An unwritable predicate is refused
- **WHEN** a user without write on `salary` sets it
- **THEN** the request is refused with `PERMISSION_DENIED` naming `salary`

### Requirement: Guardians and the superadmin
Guardians SHALL bypass predicate permissions in their namespace, SHALL be unable to delete groot or the guardians
group, and the guardians of namespace 0 SHALL be the superadmin, who alone creates, drops and lists namespaces and
allocates IDs.

#### Scenario: Deleting groot is refused
- **WHEN** a guardian deletes the groot user
- **THEN** the request is refused

#### Scenario: A guardian of another namespace is not the superadmin
- **WHEN** a guardian of namespace 2 calls `CreateNamespace`
- **THEN** it is refused with `PERMISSION_DENIED`

### Requirement: Users and groups are managed through DQL
Users, groups, memberships and rules SHALL be stored as the nodes and predicates Dgraph uses (`dgraph.xid`,
`dgraph.password`, `dgraph.user.group`, `dgraph.acl.rule`, `dgraph.rule.predicate`, `dgraph.rule.permission`) and
managed through one-request DQL mutations and the `ultragraph acl` command.

#### Scenario: A group rule takes effect
- **WHEN** a guardian adds a group with read on `name`, adds a user to it, and the user queries `name`
- **THEN** the query returns `name`

### Requirement: The namespace comes from the token
A request's namespace SHALL be the namespace in its access JWT; a namespace a client sends in metadata SHALL be
ignored.

#### Scenario: Client namespace metadata is ignored
- **WHEN** a user of namespace 1 sends namespace 2 in request metadata and queries
- **THEN** the query reads namespace 1
