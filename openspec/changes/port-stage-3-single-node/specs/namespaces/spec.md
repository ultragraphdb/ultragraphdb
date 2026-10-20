## ADDED Requirements

### Requirement: Namespaces are created, listed and dropped by the superadmin
`CreateNamespace` SHALL lease a namespace ID, write the initial schema and types into it, and create its guardians group
and groot user with the default password; `ListNamespaces` SHALL list them; `DropNamespace` SHALL refuse namespace 0 and
succeed for an unknown namespace without effect. With ACL on these calls SHALL need the superadmin, and with ACL off the
admin source and token.

#### Scenario: A created namespace has its own groot
- **WHEN** the superadmin creates a namespace and groot logs into it with the default password
- **THEN** the login succeeds and the namespace holds the initial schema

#### Scenario: Namespace 0 cannot be dropped
- **WHEN** the superadmin drops namespace 0
- **THEN** it is refused

#### Scenario: Dropping an unknown namespace succeeds without effect
- **WHEN** the superadmin drops a namespace ID never created
- **THEN** the call succeeds and no other namespace changes

### Requirement: Namespaces are isolated
Data, schema and permissions of one namespace SHALL be invisible to requests in another.

#### Scenario: One predicate name in two namespaces holds two values
- **WHEN** `name` of `0x1` is set to `A` in namespace 1 and to `B` in namespace 2
- **THEN** each namespace's query returns its own value

#### Scenario: A dropped namespace's data is gone
- **WHEN** a namespace holding data is dropped
- **THEN** no request reads its data, and a namespace created afterwards holds none of it
