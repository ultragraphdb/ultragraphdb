## ADDED Requirements

### Requirement: A transaction's changes are assembled into instances and validated whole
In a governed tenant the partition SHALL assemble the quad changes and instance operations of a transaction into
instances before it validates them: the changes sharing a subject form the writes to that instance, a subject being
an entity's IRI, a node-encoded relation instance's IRI, or an edge-encoded instance's triple with its reifier. Each
instance a transaction creates or changes SHALL be validated whole against the catalog version in effect at the
commit's epoch, whatever the order and the number of frames its changes arrived in. A violation SHALL refuse the
whole transaction with code 7 (Refused), `detail` the `UGS1xxx` number of `SCHEMA.md` section 8.7, the message naming
the instance by its IRI and the rule, and nothing of the transaction SHALL be held. A reifier written without its triple, a role triple without its instance's type and a triple of a node-encoded
relation outside its instance SHALL be refused as an incomplete instance (`UGS1034`). A transaction SHALL create an
instance when it writes `rdf:type` on a blank node or on the IRI a keyed type mints; an instance created under a blank
node SHALL be created whole in one Write. A commit SHALL report one refusal: the first rule broken in the order of
`SCHEMA.md` section 8.2's table, a value's type before any check that reads it, for the first instance in the order
its changes arrived. Unless a scenario of this capability names partitions, one partition holds the tenant's run.

#### Scenario: Triples in any order form one instance
- **WHEN** one Write holds, for a blank node, a `Supply`'s `av:Supply_item` triple, then its attributes, then its
  `rdf:type`, then its `av:Supply_supplier` and `av:Supply_buyer` triples
- **THEN** the commit succeeds with one `Supply` instance

#### Scenario: A refusal names the instance and writes nothing
- **WHEN** one transaction creates a valid `Company` and a `Claim` without its required `statement`
- **THEN** the commit is refused with code 7, `detail` 1005, the message naming the `Claim`'s IRI and `statement`,
  and a read finds neither instance

#### Scenario: A role triple without its instance is incomplete
- **WHEN** a transaction writes `_:s av:Supply_buyer <…#Organization/Leonardo>` and nothing else of `_:s`
- **THEN** the commit is refused with `UGS1034`

### Requirement: An instance has one concrete type for life
A created instance SHALL carry one concrete declared type of its graph's schema: none or an unknown one SHALL be
refused with `UGS1002`, an abstract one with `UGS1003`, and two in one creation, or a type written on an existing
keyless instance, with `UGS1004`. The instance's ID SHALL take that type's prefix, and no `rdf:type` quad SHALL be stored for it. An
entity SHALL belong to the graph of the write that creates it, a graph of its type's schema (`UGS1031`).

#### Scenario: An abstract type is refused
- **WHEN** a transaction creates an instance with `rdf:type db:Agent`
- **THEN** the commit is refused with `UGS1003` naming `Agent`

#### Scenario: Two types in one creation are refused
- **WHEN** a transaction creates an instance with `rdf:type db:Company` and `rdf:type db:Person`
- **THEN** the commit is refused with `UGS1004` naming `Company` and `Person`

#### Scenario: A subject without a type is refused
- **WHEN** Gen5 is in effect and a transaction writes `db:label` on a new subject and no `rdf:type`
- **THEN** the commit is refused with `UGS1002`

#### Scenario: An entity in a graph of another schema is refused
- **WHEN** a tenant holds two schemas and a transaction creates an aviation `Engine` in a graph of the other
- **THEN** the commit is refused with `UGS1031`

#### Scenario: The prefix carries the type
- **WHEN** a `Company` is created under Gen5
- **THEN** its system ID's type prefix is `Company`'s, the store holds no `rdf:type` quad for it, and a read of its
  `rdf:type` answers `db:Company` and each supertype

### Requirement: Required attributes, defaults and single values
A required attribute SHALL be present when its instance commits, and a removal leaving it absent SHALL be refused
(`UGS1005`). A required field with a default SHALL take the default when a write omits it. A required collection
SHALL hold at least one member. A second value written to a single-valued attribute of one instance SHALL be refused
(`UGS1036`). A `Set` SHALL hold its distinct members, a duplicate collapsing without a refusal.

#### Scenario: Defaults are applied at creation
- **WHEN** a `Company` is created with `id` `ORG:acme` alone
- **THEN** it reads back with `inferred` `false` and `rank` `normal`

#### Scenario: A required attribute cannot be removed
- **WHEN** a transaction deletes every `av:commissioned` statement of an `Engine`
- **THEN** the commit is refused with `UGS1005` naming `commissioned`

#### Scenario: A second value of a single-valued attribute is refused
- **WHEN** a transaction inserts a second `av:commissioned` value on an existing aviation `Engine` without deleting the
  present one
- **THEN** the commit is refused with `UGS1036`

#### Scenario: A set keeps its distinct members
- **WHEN** an `Engine` is created with `tags` `"hot"`, `"hot"` and `"spare"`
- **THEN** it reads back with two `av:tags` triples

### Requirement: Values are of their attribute's type
A written value SHALL be of its attribute's type as `SCHEMA.md` section 5 defines it: a datatype mismatch, an integer
outside its type's range, `NaN` or an infinity, a `Geo` value that is no GeoJSON `Point`, `LineString` or `Polygon` in
WGS 84, and an interval whose start is after its end or with both bounds open SHALL be refused with `UGS1011`; an
undeclared enum member with `UGS1030`; a vector of another dimension with `UGS1028`; an undeclared unit, or a unit of
another quantity kind, with `UGS1027`. A `Measure<K, u>` SHALL be stored as `xsd:double` in `u`, a `Measure<K>` and a
`Measure` as `ugdb:measure` with the unit on the value. An interval SHALL be stored as its bounds at the unit's first
instant, with a precision per bound where coarser than its type's. A `Vector<F32>` value SHALL be stored as a value; a write of a `Vector<BF16>` value SHALL be refused with code 7, `detail` the term's index, the message naming stage 8.
A catalog type held in an attribute SHALL be a declared type at or below its bound (`UGS1011`).

#### Scenario: A unit of another kind is refused
- **WHEN** a Gen5 `Event` is written with `altitude` `"100 [kn_i]"^^ugdb:measure`
- **THEN** the commit is refused with `UGS1027` naming `kn` and `Altitude`

#### Scenario: A measure in a declared unit of its kind is stored with its unit
- **WHEN** a Gen5 `Event` is written with `altitude` `"35000 [ft_i]"^^ugdb:measure`
- **THEN** it reads back with that literal, and a range filter in metres finds it at 10,668

#### Scenario: A month bound keeps its precision
- **WHEN** a `Lease` is written with `term` from the month `2026-09` to the day `2027-03-15`
- **THEN** `av:term_start` is `2026-09-01`, `av:term_start_precision` is `month`, `av:term_end` is `2027-03-15`, and
  no `av:term_end_precision` is stored

#### Scenario: An interval running backwards is refused
- **WHEN** a `Lease` is written with `term` `2027-01-01 .. 2026-01-01`
- **THEN** the commit is refused with `UGS1011`

#### Scenario: A non-finite float is refused
- **WHEN** an `Engine` is written with `thrust_kn` `"NaN"^^xsd:double`
- **THEN** the commit is refused with `UGS1011`

#### Scenario: A geometry outside the three kinds is refused
- **WHEN** a Gen5 `Space` is written with `where` a GeoJSON `MultiPolygon`
- **THEN** the commit is refused with `UGS1011`

#### Scenario: An undeclared enum member is refused
- **WHEN** a `Company` is written with `rank` `"best"`
- **THEN** the commit is refused with `UGS1030` naming `Rank`

#### Scenario: A vector of another dimension is refused
- **WHEN** an aviation `Engine` is written with an `embedding` of 767 `F32` elements
- **THEN** the commit is refused with `UGS1028` naming 767 and 768

#### Scenario: A BF16 vector waits for stage 8
- **WHEN** a Gen5 `Object` is written with an `embedding` of 768 `BF16` elements
- **THEN** the write is refused with code 7, `detail` the term's index, the message naming stage 8, and an `Object`
  written without `embedding` commits

### Requirement: Value checks hold at every commit of the instance
`@pattern`, `@range`, `@values` and `@check` SHALL hold at every commit of an instance whose checked values changed,
`@pattern` matching the whole value, and a record field's checks whenever an instance holding the record commits; a
value breaking one SHALL be refused with `UGS1010`, the message naming the annotation. Expressions SHALL be evaluated
as `SCHEMA.md` section 4 states: logic two-valued, a comparison with an absent operand false and arithmetic with one
absent; a check on an attribute or a record field holding vacuously of an absent optional value; a `@check` on a type
evaluated on the instance as written. Evaluation SHALL fail closed: a check or a `@limit` condition whose evaluation
overflows or divides by zero, and a computed derivation's, SHALL refuse the commit with `UGS1039`, the message naming
the annotation or the derivation and the fault; `UGS1010` SHALL be a check that evaluated and did not hold.

#### Scenario: A pattern is anchored
- **WHEN** a `Company` is created with `id` `xORG:acme`, which the pattern matches only unanchored
- **THEN** the commit is refused with `UGS1010` naming `@pattern` and `id`

#### Scenario: A range bounds a value
- **WHEN** a `Claim` is written with `confidence` 1.5
- **THEN** the commit is refused with `UGS1010` naming `@range(0, 1)`

#### Scenario: An attribute's check holds of an absent value
- **WHEN** a `Claim` is written without `confidence`, whose declaration is `Float64 Optional @range(0, 1)`
- **THEN** the commit succeeds

#### Scenario: An unguarded type check refuses an absent value
- **WHEN** a type declares `notes: Text Optional` and `@check(notes <> "")`, and an instance is written without `notes`
- **THEN** the commit is refused with `UGS1010` naming the check

#### Scenario: A check that overflows refuses the commit
- **WHEN** a type declares `a: Int64`, `b: Int64` and `@check(a + b > 0)`, and an instance is written with both at
  9223372036854775807
- **THEN** the commit is refused with `UGS1039`, the message naming the check and the overflow

#### Scenario: A type check reads several fields
- **WHEN** one transaction creates a Gen5 `Reading` with `valid` and neither `value` nor `text`, and a `GeneratedBy`
  naming it and an existing `Activity`
- **THEN** the commit is refused with `UGS1010` naming `@check(value is some or text is some)`

#### Scenario: A check reads a participant's type
- **WHEN** a `ResponsibleFor` is inserted with a `Software` party and `raci` `accountable`
- **THEN** the commit is refused with `UGS1010`

#### Scenario: A values list bounds a value
- **WHEN** an `Authority` is created with `jurisdiction` `"JP"`
- **THEN** the commit is refused with `UGS1010` naming `@values`

### Requirement: Keys, unique values and immutable values
A keyed instance SHALL be created with its key (`UGS1006`); a write of another value to a key attribute SHALL be
refused (`UGS1007`), and a write replacing a present `@immutable` value with `UGS1012`. A key SHALL be unique across
its declaring type's tree per graph (`UGS1008`), and a `@unique` value across its declaring type's tree per graph when
present (`UGS1009`), each decided at the value's claim. A claim of a `@bitemporal` or `@append_only` type SHALL be
permanent: a retracted instance keeps its key, its `@unique` values and its IRI, and no later instance is given them.
A deleted instance of a mutable type SHALL release its claims. A claim owned by another partition than the
transaction's SHALL refuse the transaction with code 23.

#### Scenario: A keyed instance is created with its key
- **WHEN** a transaction creates a `Company` under `…#Node/ORG%3Aacme` without the `db:id` quad
- **THEN** the commit is refused with `UGS1006`, the key never being read out of the IRI

#### Scenario: A key is unique across its tree
- **WHEN** a `Company` with `id` `ORG:acme` exists and a transaction creates a `Person` with `id` `ORG:acme`
- **THEN** the commit is refused with `UGS1008` naming `Node` and the key

#### Scenario: A retracted instance keeps its key
- **WHEN** the `Company` `ORG:acme` is retracted and a transaction creates a `Company` with `id` `ORG:acme`
- **THEN** the commit is refused with `UGS1008`

#### Scenario: A deleted mutable instance releases its key
- **WHEN** the aviation `Engine` `EN-4471-B` is deleted and a transaction creates an `Engine` with that serial
- **THEN** the commit succeeds, and the new instance has a new system ID

#### Scenario: A unique value is refused twice
- **WHEN** a `Context` holds `context_id` 7 and a transaction creates another `Context` with `context_id` 7
- **THEN** the commit is refused with `UGS1009`

#### Scenario: An immutable value is not replaced
- **WHEN** a schema declares the mutable `entity Device @key(id) { id: String  model: String @immutable }` and a
  transaction replaces the `model` of an existing `Device`, deleting the present value and inserting another
- **THEN** the commit is refused with `UGS1012`

#### Scenario: A key attribute is not rewritten
- **WHEN** a transaction replaces the `serial` of an existing aviation `Engine`, deleting the present value and
  inserting another
- **THEN** the commit is refused with `UGS1007`

### Requirement: Participants and roles
A relation instance SHALL be created with every required role filled and every set-valued role non-empty
(`UGS1015`, `UGS1016`), each participant existing (`UGS1017`), live (`UGS1019`) and of the role's type or union
(`UGS1018`); a removal emptying a set-valued role SHALL be refused (`UGS1016`); a role naming a catalog type SHALL hold
a declared type at or below its bound (`UGS1018`) and take no existence or liveness check; a participant being an entity, a node-encoded instance or an edge-encoded instance named by its reifier.
A participant SHALL change only in an `Optional` or set-valued role: a write replacing the participant of a required,
single-valued role SHALL be refused with `UGS1025`, on a relation of any mutability class. A participant in another
partition SHALL be checked through Reference's get-only mode, which answers its status, absent, live, retracted or
deleted, and its type prefix, before the commit.

#### Scenario: A participant outside the role's union is refused
- **WHEN** a `MemberOf` is inserted with a `Claim` as `member`
- **THEN** the commit is refused with `UGS1018` naming `member`, `Claim` and `Endurant | Gap`

#### Scenario: A participant that does not exist is refused
- **WHEN** a `MemberOf` is inserted whose `collection` names `…#Node/ORG%3Anone`, which no instance holds
- **THEN** the commit is refused with `UGS1017`

#### Scenario: A retracted participant is refused
- **WHEN** the `Company` `ORG:acme` is retracted and a `MemberOf` is inserted naming it as `collection`
- **THEN** the commit is refused with `UGS1019`

#### Scenario: A missing required role is refused
- **WHEN** a `Supply` is created with `supplier` and `buyer` and no `item`
- **THEN** the commit is refused with `UGS1015` naming `item`

#### Scenario: A required role's participant is fixed
- **WHEN** a transaction replaces the `lessee` triple of an existing aviation `Lease` by one naming another
  organization
- **THEN** the commit is refused with `UGS1025` naming `Lease` and `lessee`

#### Scenario: A required role is fixed on a bitemporal relation too
- **WHEN** a transaction replaces the `member` triple of an existing Gen5 `MemberOf`
- **THEN** the commit is refused with `UGS1025`, and a transaction setting its optional `role` commits as a revision

#### Scenario: A relation over a relation names its reifier
- **WHEN** a Gen5 `DependsOn` instance exists and an `AssertedBy` is inserted with it as `statement`
- **THEN** the commit succeeds, and the `AssertedBy` triple's subject is the `DependsOn` instance's reifier, named by
  its handle IRI

#### Scenario: A relation over a node-encoded relation names its node
- **WHEN** a `MemberOf` instance exists and an `AssertedBy` is inserted with it as `statement`
- **THEN** the commit succeeds, and the `AssertedBy` triple's subject is the `MemberOf` instance's IRI

#### Scenario: A set-valued role is not emptied
- **WHEN** a transaction removes the only `members` participant of a Gen5 `Alternatives`
- **THEN** the commit is refused with `UGS1016`

#### Scenario: A catalog type outside its bound is refused
- **WHEN** a Gen5 `Absence` is inserted whose `kind` names `db:AssertedBy`, which is not below `Relation`
- **THEN** the commit is refused with `UGS1018`

#### Scenario: A participant in another partition is checked by reference
- **WHEN** a relation instance anchored in partition 1 names a live participant of the role's type in partition 2
- **THEN** partition 2 answers the reference in get-only mode with status live and the participant's type prefix, and
  the commit succeeds

### Requirement: Cardinality, set and multi relations, symmetric relations
A navigation name's cardinality SHALL bound the live instances one participant takes part in through its role, checked
at the anchor's partition for the anchor role: an insert, delete or retraction taking the count out of bounds SHALL be
refused with `UGS1014`, a lower bound being checked at the commit that creates the participant or removes an
instance. A `@set` relation SHALL hold at most one live instance per combination of participants: an insert of an
existing combination SHALL resolve to the existing instance. A `@multi` relation SHALL create an instance per insert.
A `@symmetric` relation SHALL store one instance for `(a, b)` and `(b, a)`, the participant with the lower system ID
first, reached from either participant.

#### Scenario: An upper bound on the anchor role
- **WHEN** an asset has an `Owns` instance and a transaction inserts an `Owns` for it with another owner
- **THEN** the commit is refused with `UGS1014` naming `owner?`

#### Scenario: A lower bound is met in the creating transaction
- **WHEN** one transaction creates a `Post` and a `PostIn` naming it and an existing `Organization`
- **THEN** the commit succeeds; and a transaction creating a `Post` alone is refused with `UGS1014` naming
  `organisation{1,1}`

#### Scenario: A set insert is idempotent
- **WHEN** `AssertedBy` is inserted twice for one statement and one asserter, in two transactions
- **THEN** both commit, and one instance exists

#### Scenario: A multi relation holds parallel instances
- **WHEN** `MemberOf` is inserted twice for one member and one collection
- **THEN** two instances exist, each with its own IRI

#### Scenario: A symmetric set relation is one instance in either order
- **WHEN** a schema declares the `@symmetric` relation `Adjacent` between two `Space` roles sharing the navigation
  name `adjacent`, and `(a, b)` then `(b, a)` are inserted
- **THEN** one instance exists, stored from the participant with the lower ID, and `adjacent` reaches it from `a` and
  from `b`

### Requirement: Acyclic relations, limits and no-overlap
`@acyclic` SHALL refuse an insert or participant change closing a cycle through the relation tree's two roles
(`UGS1021`), and `max_depth` one creating a longer path (`UGS1022`), walking the relation's paths from the inserted
instance within the partition, and under `open` counting only live instances whose valid interval is open. `@limit`
SHALL hold per combination of the `per` participants, and per concrete subtype under `type`, over the live instances
satisfying its condition, a top-level conjunct over the `per` participants' types selecting the groups; a write
breaking it SHALL be refused with `UGS1023`, a lower bound at the commit creating the grouped participant, and a condition that cannot be evaluated with `UGS1039`.
`@limit` SHALL be checked at an update of an attribute its condition reads as at an update of the attribute it
aggregates. Each group's aggregate SHALL be a record in the marks keyspace of the partition owning the group, written
in the commit that moves it, and a check SHALL read that record and no instance of the group.
`@no_overlap` SHALL refuse an insert or update after which two live instances sharing the `per` participants have
valid intervals that certainly overlap, or with `strict` possibly overlap (`UGS1024`). A check of `@acyclic`, with or
without `max_depth`, or of `@no_overlap` SHALL read at most `rule_read_max` instances, 4,096, and one that would read
more SHALL be refused with `UGS1040`, naming the rule and the bound. A check whose instances or paths lie in another
partition SHALL refuse the transaction with code 23.

#### Scenario: A cycle is refused
- **WHEN** `SubOrganizationOf` holds open instances making `b` a unit of `a` and `c` a unit of `b`, and one making `a`
  a unit of `c` is inserted
- **THEN** the commit is refused with `UGS1021`

#### Scenario: A path past max_depth is refused
- **WHEN** aviation assets form a `PartOf` chain of 16 references and a 17th is appended
- **THEN** the commit is refused with `UGS1022` naming 16

#### Scenario: One preferred instance per subject and type
- **WHEN** a `Document` has an `IsAbout` instance with `rank` `preferred` and a second `IsAbout` with `rank` `preferred`
  is inserted for it
- **THEN** the commit is refused with `UGS1023`; and a `Realizes` with `rank` `preferred` for the same subject commits

#### Scenario: A revision to preferred is checked
- **WHEN** a `Document` has an `IsAbout` instance with `rank` `preferred` and a second `IsAbout` with `rank` `normal`,
  and a transaction revises the second's `rank` to `preferred`
- **THEN** the commit is refused with `UGS1023`, and both instances read as they were

#### Scenario: A limit reads its group's aggregate
- **WHEN** a `Document` has 1,000 `IsAbout` instances with `rank` `normal`, and one with `rank` `preferred` is inserted
- **THEN** the commit succeeds having read the group's one aggregate record and none of its instances, and the record
  then counts one

#### Scenario: A walk past its bound is refused
- **WHEN** a relation of the corpus declared `@acyclic` without `max_depth` holds a chain of 4,096 instances in one
  partition, and an instance extending the chain is inserted
- **THEN** the commit is refused with `UGS1040` naming `@acyclic` and 4,096, and nothing of it is written

#### Scenario: A lower bound selected by type
- **WHEN** a transaction creates a `Reading` with `valid` and a `value` and no `GeneratedBy`, and another creates a
  `Claim` without one
- **THEN** the first is refused with `UGS1023`, and the second commits

#### Scenario: A sum limit
- **WHEN** a buyer's live `Supply` instances sum to 49,000,000.00 and a `Supply` of 2,000,000.00 is inserted for it
- **THEN** the commit is refused with `UGS1023` naming the buyer and the bound

#### Scenario: One holder of a post at a time
- **WHEN** a `Holds` with `valid` from `2026-01-01T00:00:00Z` and no end exists for a post, and a second `Holds` for
  that post with `valid` from `2026-06-01T00:00:00Z` is inserted
- **THEN** the commit is refused with `UGS1024`

### Requirement: Mutability classes
An instance of a mutable type SHALL be updatable and deletable. An instance of an `@append_only` type SHALL be
inserted and never updated or deleted, and on a subtype of a `@bitemporal` type never retracted (`UGS1013`). A client
write naming a derived name SHALL be refused (`UGS1029`).

#### Scenario: An append-only instance is not updated
- **WHEN** a transaction writes another `value` on an existing aviation `Supply`
- **THEN** the commit is refused with `UGS1013`

#### Scenario: An append-only link is not deleted
- **WHEN** a transaction deletes an `AssertedBy` triple
- **THEN** the commit is refused with `UGS1013`

#### Scenario: An append-only bitemporal instance is not retracted
- **WHEN** a transaction retracts a `Reading`
- **THEN** the commit is refused with `UGS1013`

#### Scenario: A derived name is not written
- **WHEN** a transaction writes `db:closed` on a `Company`
- **THEN** the commit is refused with `UGS1029`

### Requirement: Bitemporal statements
An instance of a `@bitemporal` type SHALL carry `asserted`, stored as the epoch of the commit inserting it and, once
retracted, the epoch of the commit retracting it, read as the sequencer's close times, and `live`, the absence of
the end. An update of its attributes or of an optional or set-valued role SHALL be a revision: a new version of the
same instance with the same identity, key and IRI, every version kept. A retraction SHALL close `asserted` and keep
the instance readable; a retraction of an instance of a type that is not `@bitemporal` SHALL be refused
(`UGS1026`), and a delete of a `@bitemporal` instance SHALL be a retraction. A default read SHALL see live instances;
a read `with retracted` every instance. Constraints SHALL count live instances, keys and `@unique` retracted ones too.
Instances of `@bitemporal` and `@append_only` types SHALL be kept beyond the retention window.

#### Scenario: An update is a revision
- **WHEN** the `Company` `ORG:acme` with `label` `Acme` is updated to `label` `Acme Corp` in a later epoch
- **THEN** the default read answers `Acme Corp` under the same IRI, `asserted_start` is unchanged, and a read at the
  earlier epoch answers `Acme`

#### Scenario: A revision is refused under its rule's own number
- **WHEN** three transactions each update a `@bitemporal` instance: one writes another `id` on a `Company`, one
  another `supertype`, which is `@immutable`, on a `ProposedKind`, and one another `outcome` on a `Verdict`, which is
  `@append_only`
- **THEN** they are refused with `UGS1007`, `UGS1012` and `UGS1013`, in that order

#### Scenario: A retraction closes asserted and keeps the instance
- **WHEN** the `Company` `ORG:acme` is retracted
- **THEN** a default read does not find it, a read `with retracted` finds it with `ugdb:live` `false` and an
  `ugdb:asserted_end`, and every attribute it held is still read

#### Scenario: A mutable instance is not retracted
- **WHEN** a transaction retracts an aviation `Engine`
- **THEN** the commit is refused with `UGS1026`

#### Scenario: History outlives the retention window
- **WHEN** a `Company` is revised, and the retention window passes
- **THEN** a read at the epoch of its first version still answers that version

### Requirement: Retraction reaches relations as declared
`@on_retract(role: action)` SHALL govern a participant's retraction for `@bitemporal` relations, `cascade` by default:
`cascade` retracts the live instances naming the participant in that role in the same commit; `restrict` refuses the
retraction while live instances name it; `restrict open` refuses it while open instances name it and retracts, in the
same commit, the live instances that are no longer open (`UGS1020`). An instance of a relation that is not
`@bitemporal` SHALL be live while every participant of it is live, derived and never written by a retraction.
`@retracts(role)` SHALL retract the participant in that role in the commit inserting the instance, the participant
being live (`UGS1019`), and the instance SHALL stay live naming it. `@ends(role)` SHALL close the participant's valid
interval at the instance's own valid start, with that bound's precision, in the commit inserting the instance, the
instance stating a valid start and the participant being live with a valid interval beginning before that start and
open or ending after it (`UGS1035`). A cascade or a restrict reaching instances in another partition SHALL refuse the
transaction with code 23.

#### Scenario: An open membership restricts its collection's retraction
- **WHEN** an `Organization` has, as `collection`, a live `MemberOf` whose `valid` has no end, and the organization is
  retracted
- **THEN** the commit is refused with `UGS1020` naming `MemberOf` and `collection`

#### Scenario: A closed membership is retracted with its collection
- **WHEN** an `Organization`'s only live `MemberOf`, as `collection`, has a `valid` that states an end, and the
  organization is retracted
- **THEN** the commit succeeds, the membership is retracted in the same commit, and a read believed at an earlier
  epoch finds both

#### Scenario: A cascade retracts bitemporal relations
- **WHEN** a `Context` named by live `InContext` instances is retracted
- **THEN** the commit succeeds and those instances are retracted in the same commit

#### Scenario: An append-only link follows its participant's liveness
- **WHEN** a `Claim` with an `AssertedBy` link is retracted
- **THEN** the link is not written, a default read of `asserted_by` finds nothing for the claim, and a read `with
  retracted` finds the link

#### Scenario: A correction retracts what it supersedes
- **WHEN** a `Supersedes` is inserted with a new `Claim` as `new` and a live `Claim` as `old`
- **THEN** the old claim is retracted in the same commit, and the `Supersedes` instance is live naming it

#### Scenario: A succession ends the old statement's validity
- **WHEN** a `Replaces` with `valid` starting at the day `2026-07-01` is inserted naming an old statement whose `valid`
  began in 2025 and is open
- **THEN** the old statement stays live, its `valid` ends at `2026-07-01` with precision `day`; and a `Replaces`
  without a `valid` start is refused with `UGS1035`

### Requirement: Deletes
A delete of an entity of a mutable type SHALL remove the entity and apply each relation's `@on_delete` action to the
instances naming it: `cascade`, the default, deletes them and the relation instances over them in the same commit;
`restrict` refuses the delete while instances name the participant in that role (`UGS1020`); `orphan` leaves the
instance with its optional role empty. The delete SHALL write a tombstone at the entity's home, write-set operation
8: a later commit referencing the deleted entity's ID SHALL abort with code 30 (EntityDeleted), a reference in
get-only mode SHALL answer deleted, and a write creating its IRI again SHALL allocate a new ID. A delete of a relation instance SHALL remove the instance and cascade to the relation instances over it; an
edge-encoded instance that holds no reifier SHALL be deleted by the delete of its quad. A delete and a re-creation
SHALL leave the new instance's record naming the ID it replaced, so a read naming the IRI at a version below the
delete resolves the instance that held it then. A delete whose
entity's referrer set names a bucket group another partition holds SHALL be refused with code 23.

#### Scenario: A delete cascades by default
- **WHEN** an aviation `Aircraft` with an `Owns` instance and no other relation is deleted
- **THEN** the commit succeeds, and neither the aircraft nor the `Owns` instance is read

#### Scenario: A restricted participant is not deleted
- **WHEN** an `Engine` that is the `whole` of a `PartOf` instance is deleted
- **THEN** the commit is refused with `UGS1020` naming `PartOf` and `whole`

#### Scenario: A tombstone fences the deleted ID
- **WHEN** an `Aircraft` is deleted, and a transaction that resolved its ID before the delete commits a reference to it
- **THEN** that commit aborts with code 30, the message naming the aircraft's IRI

#### Scenario: A read of the past finds a recreated entity's predecessor
- **WHEN** an aviation `Engine` `EN-4471-B` with `thrust_kn` 310 is deleted at epoch 20 and created again with
  `thrust_kn` 400 at epoch 22
- **THEN** a read naming its IRI at epoch 19 answers 310, and a read at epoch 23 answers 400

#### Scenario: Orphan empties an optional role
- **WHEN** a relation declares `@on_delete(reviewer: orphan)` on its `Optional` role and the reviewer is deleted
- **THEN** the instance stays, with no `reviewer` triple

### Requirement: Indexes are written in the instance's local commit
`@index(range)`, `@index(interval)`, `@index(geo)` and `@index(fulltext)` SHALL be maintained in the instance's
partition in the same local commit as the write, with the tokenizers `SCHEMA.md` section 6.10 names: an interval index
as two range entries over the outer extent, an index on a `Set` one entry per member, a measure of a kind by its value
in the canonical unit. A count index SHALL serve every navigation name with a bounded cardinality, and the unique flag
every `@key` and `@unique` predicate. `@index(ann)` SHALL be recorded and no index built.

#### Scenario: A range index over a measure orders by the canonical unit
- **WHEN** two Gen5 `Event`s hold `altitude` `"1000 m"` and `"2000 [ft_i]"`
- **THEN** the range index orders the second, 609.6 m, before the first

#### Scenario: An interval index holds the outer extent
- **WHEN** a statement's `valid` runs from the month `2026-09` to the year `2027`
- **THEN** its interval index entries are `2026-09-01T00:00:00Z` and `2028-01-01T00:00:00Z`

#### Scenario: A fulltext index follows the language tag
- **WHEN** a `Claim` holds `statement` `"running engines"@en`
- **THEN** the fulltext index lists it under the English stems of `running` and `engines`

#### Scenario: A geo index covers a point
- **WHEN** a `Space` holds `where` a GeoJSON `Point`
- **THEN** the geo index lists it under the point's S2 cell tokens, as the store's geo tokenizer derives them

#### Scenario: A line string is indexed by its cover
- **WHEN** a Gen5 `Space` is written with `where` a GeoJSON `LineString`
- **THEN** the commit succeeds, and the geo index lists it under the tokens of its cover

#### Scenario: No vector index is built
- **WHEN** an aviation `Engine` is written with a 768-element `F32` `embedding`
- **THEN** the value is stored, and the store holds no vector index entry for `av:embedding`

### Requirement: Materialized derivations are maintained with the write
A `@materialized` derivation, every computed derivation by default, SHALL be stored as quads under the derived name's
predicate and maintained in every local commit that changes it, a retraction included, within the anchor partition;
`@index(range)` on it SHALL be maintained likewise. A computed derivation whose evaluation fails SHALL refuse the
commit with `UGS1039`. A `@virtual` derivation SHALL be recorded and not stored.

#### Scenario: Gen5's closed flag follows the statement
- **WHEN** a `Company` is created with no `valid` end and `rank` `normal`, then its `rank` is set to `deprecated`
- **THEN** `db:closed` reads `false` after the first commit and `true` after the second, and the range index of
  `db:closed` lists it under `true`

#### Scenario: A retraction maintains the flag
- **WHEN** a `Company` with `closed` `false` is retracted
- **THEN** a read `with retracted` answers `db:closed` `true`

### Requirement: A cross-partition rule is refused until stage 8
A write SHALL abort with code 23 (CrossPartition), writing nothing, the message naming the partitions, when its
validation must read or change state another partition owns, beyond a participant's status and type answered by
reference:
a key's or `@unique` value's claim at another partition, a bound or `@limit` whose instances are spread, an
`@acyclic` path crossing partitions, a `restrict`, `cascade`, `@retracts` or `@ends` reaching another partition.

#### Scenario: A unique claim owned elsewhere
- **WHEN** partition 1 owns the bucket of `context_id` 7's claim and a transaction bound to partition 2 creates a
  `Context` with `context_id` 7
- **THEN** the transaction aborts with code 23 naming partitions 2 and 1

#### Scenario: A participant elsewhere does not refuse a coordination-free insert
- **WHEN** a `DependsOn` anchored in partition 1 names a live `dependency` in partition 2, with `rank` `normal`
- **THEN** the commit succeeds
