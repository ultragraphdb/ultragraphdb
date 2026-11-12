## MODIFIED Requirements

### Requirement: IRIs and blank nodes get system IDs
Each IRI and each skolemized blank node, other than a reifier's skolem IRI, SHALL map, per tenant, to one system ID of
the `Resource` type, never reused: an IRI's bucket computed from the tenant and the IRI's canonical encoding, a blank
node's the one its skolem IRI states, and its sequence drawn from a block its home holds. The same IRI SHALL always map
to the same ID, and its dictionary entries SHALL be held in its home, the partition owning its bucket.

A new blank node's skolem IRI SHALL be the tenant's skolem base, `https://w3id.org/ultragraphdb/ns/tenant/{n}/` with `{n}` the tenant in decimal, followed
by a bucket and 104 random bits, as `https://w3id.org/ultragraphdb/ns/tenant/{n}/.well-known/genid/{bucket}-{random}`, the bucket in 6 and the random bits
in 26 lowercase hexadecimal digits. The
partition minting it SHALL draw both from its injected random source: the bucket from those it owns in the tenant's
run, the random bits uniformly. A skolem IRI that names an IRI the dictionary already holds SHALL be drawn again. The
skolem IRI SHALL hold no part of the system ID beyond its bucket. A transaction whose first write names only new blank
nodes SHALL bind to a partition compute chooses in turn.

A statement ID's reifier SHALL be named by a skolem IRI minted as a blank node's: the statement's bucket, the bucket of
its subject, and 104 random bits from the subject's partition's injected random source, drawn again when the IRI is
already held. The dictionary SHALL hold that IRI as the reifier's name, mapping it to the statement ID and back, so a
client addresses the statement's properties, or one of several parallel edges, through it.

#### Scenario: A statement ID's reifier is a skolem IRI
- **WHEN** `<a> <knows> <b>` gains the property `<since> 2020` and so receives a statement ID
- **THEN** the dictionary holds a skolem IRI `…/tenant/{n}/.well-known/genid/{bucket}-{random}` for it, whose bucket is `<a>`'s,
  mapping to the statement ID and back, and a write naming that IRI as the reifier adds properties to the statement

#### Scenario: An IRI keeps its ID
- **WHEN** `<a>` is written in two separate commits
- **THEN** both name the same system ID

#### Scenario: Transactions naming only new blank nodes bind in turn
- **WHEN** three transactions whose first writes name only new blank nodes arrive over three partitions
- **THEN** compute binds them to partitions 1, 2 and 3 in turn, and each blank node's bucket is one its partition
  owns

#### Scenario: An IRI's bucket is its own
- **WHEN** a new IRI receives a system ID
- **THEN** its type prefix is 1 and its bucket equals the bucket computed from its tenant and canonical encoding

#### Scenario: A blank node's skolem IRI names its bucket and nothing else
- **WHEN** partition 1, owning the first 21,846 buckets of tenant 0's run, mints a blank node with its random source
  seeded to a recorded value
- **THEN** the skolem IRI is the tenant's skolem base, ending `/tenant/0/`, `.well-known/genid/`, 6 hexadecimal digits naming one of those
  buckets, `-` and 26 hexadecimal digits; the node's ID carries that bucket; and the same seed mints the same IRI
