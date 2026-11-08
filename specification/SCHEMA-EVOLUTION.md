# Schema evolution with data migration

* Status: first draft, 2026-11-08, for stage 9.5 (`ROADMAP.md`; `openspec/notes/backlog.md`, item 16).

This file specifies how a namespace moves from one schema version to another when the change reshapes data: a type
renamed, split or merged, an attribute moved into a relation or back, a key changed. Such a change installs as a
**migration**: the target version is created beside the source as a staged namespace, filled from the source's live
data by a declared mapping, checked against every rule of the target, and switched in at one epoch behind a write
fence. Instances keep their IRIs across it. The source stays installed, read-only, holding the history the target does
not carry.

`SCHEMA.md` gives the schema language, the catalog and the in-place changes of its section 13, which this file extends
and does not replace; `PROTOCOL.md` the Install request and the epochs a cutover takes effect at;
`QUERY-GQL-CORE.md` and `QUERY-SPARQL-CORE.md` the queries a migration's expressions are drawn from. The semantics are
those of functorial data migration, restricted to a fragment that never merges instances (section 2).

## Open questions

- Is Π needed for any migration DeepBrain foresees, or does every reshaping fit one origin per image?
- How long does the source stay readable as `{namespace}@{version}`, and what retires it: an explicit request, an age,
  or the drop of its last reader's grant?
- How does a query read across the cutover, the target's present and the source's history together, beyond UQL's
  `believed at` reaching into the source?
- How long is a rename's deprecation window, and is it the migration's to set or the tenant's?
- Does a namespace that imports the migrating one migrate with it, or must its imports name the target's version
  first?
- May a run's catch-up recompute only what changed, through the referrer sets of the instances written in `(e0, e1]`,
  or must it rescan every image whose assignments cross partitions? An extended term that reads a derivation or a fold
  makes an image depend on every instance the read reaches.
- Does a migration read several namespaces at once? Its source is then their coproduct, or the union of their graphs
  where they reference each other (`SCHEMA-APG.md`, definition 16.5).

## Contents

1. Conventions
2. The model
3. The migration declaration
4. Identity across versions
5. Checks at install
6. The run
7. History and reads across versions
8. Distribution and cost
9. Clients
10. Refusals
11. What this specification asks of the others
12. Decisions
13. Open questions
14. Sources

## 1. Conventions

| Term | Meaning |
|---|---|
| Tenant, namespace, catalog version, epoch | As `SCHEMA.md` section 1 |
| Source, target | The namespace's installed schema version a migration reads, and the version it builds |
| Migration | A `migration` source (section 3) installed with the target's schema source, and the run that carries data from the source to the target (section 6) |
| Staged namespace | The target while the run holds it: allocated, filled and checked, invisible to every request but the run's |
| Origin, image | A live source instance, and a target instance the migration builds from it. Every image has exactly one origin |
| Primary image | The one image of an origin that inherits the origin's IRI; at most one per origin |
| Key space | The identity a keyed instance's bucket, uniqueness claim and IRI are derived from: the prefix and the name of the type that first declared the key, carried through every rename and split of its declaring type (section 4.1) |
| Birth IRI | The IRI an instance was minted with; an instance keeps it for life (section 4.3) |
| Retired name | A name a type, attribute, role or navigation name held in an earlier version; the catalog keeps it (section 4.2) |
| Cutover | The epoch at which the namespace's name passes from the source to the target |
| SHALL, refused | As `SCHEMA.md` section 1; a migration refused at install changes nothing, and one refused during its run discards the staged namespace and leaves the source as it was |

Diagnostics of this file take `UGS2101` to `UGS2199` at install and `UGS2201` to `UGS2299` during the run, within
`SCHEMA.md`'s evolution range.

## 2. The model

A namespace's schema denotes an algebraic property graph schema, and its data an algebraic property graph on it
(`SCHEMA-APG.md`). A migration is a schema mapping `Φ` from the target schema to the source schema, giving each target
label an image, a type over the source's labels, and a term computing the target element's value; the target's data is
`Δ_Φ` of the source's (`SCHEMA-APG.md`, section 9), followed by the selections and key lookups below. In the
relational reading of functorial data migration, a mapping moves data three ways: Δ, which reads the source through
the mapping, Σ, which pushes source instances forward and may identify them, and Π, which forms the target's instances
from joins of source instances (Spivak, *Functorial Data Migration*, section 4).

UltraGraphDB admits the **non-merging fragment**:

- **Every image has exactly one origin.** Each target label's image is a source label, or a variant of source labels
  for a union of source types, so each target element is one source element (`SCHEMA-APG.md`, proposition 9.5). An
  image that is a record of labels would build a target instance from a join of independent source instances, Π's case
  in the relational reading, at the cost of a product over the joined types; it is outside the fragment.
- **No two origins share an image.** Distinct source elements give distinct target elements: a union of source types
  into a target type, never an identification of their instances. In the relational reading this is Σ along a
  discrete op-fibration; in the algebraic property graph it is `Δ` along a variant of labels.
- **An image is a function of its origin and the source snapshot.** Its attributes and roles are extended mapping
  terms over the origin (`SCHEMA-APG.md`, definition 16.3), read at one snapshot epoch: the pure terms of definition
  8.5 there, `map` and `filter` over collections (definition 12.4 there), and reads of the source, key lookups,
  inverse navigations, derivations, folds and functions. Proposition 9.3's composition law holds for a migration whose
  terms are all pure, which the migration report states (section 5).
- **Two steps beyond `Δ`.** A map's `where` is a selection, defined when what it keeps references nothing it drops
  (`SCHEMA-APG.md`, definition 16.1); a role assigned by key lookup searches a label's elements by key and reaches at
  most one, because a key is exclusive under PG-Keys' semantics (definition 16.2). Proposition 16.4 there states the
  fragment's semantics.

Within the fragment a migration is a union of selections, renamings and per-instance computations. Spivak and Wisnesky
show that Δ, and Σ along a discrete op-fibration, compile to selection, projection, union and key generation
(*Relational Foundations for Functorial Data Migration*, theorems 3 and 4), which is how the run executes it. Each image
is computed where its origin lives, so the run is per partition (section 8). A mapping outside the fragment is refused
at install where the compiler can see it (section 5) and during the run where only the data shows it, as two origins
whose images collide on a target key (`UGS2201`).

The fragment is what UGS's own rules ask for. A UGS write that would identify two instances, a duplicate key, is
refused at commit (`SCHEMA.md` section 8.7); a migration that would identify two is refused the same way, never
resolved by merging.

## 3. The migration declaration

### 3.1 Grammar

```
migration      = { doc } "migration" ident "from" int "to" int "{" { step } "}" ;
step           = rename | map | drop ;
rename         = "rename" kind qname "to" ident ;
kind           = "type" | "attribute" | "role" | "nav" | "graph" ;
map            = "map" qname [ "primary" ] "from" qname { "," qname } [ "where" expr ]
                 [ "{" { assign } "}" ] ;
assign         = ident "=" mexpr ;
mexpr          = "self" | expr | ( "map" | "filter" ) "(" expr "," ident "=>" expr ")" ;
drop           = "drop" kind qname ;
qname          = ident [ "." ident ] ;
```

`ident`, `int`, `doc` and `expr` are `SCHEMA.md` section 4's: `expr` is the expression grammar of checks and
derivation filters, with paths of navigation names, roles and attributes from the origin, folds, and the bitemporal
items. A migration names its namespace and the two versions; the target version's schema source travels in the same
install (section 5).

### 3.2 Semantics

- **The identity map is implicit.** A type, attribute, role, relation or navigation name whose name and declaration
  are unchanged between the versions maps to itself; a migration lists only differences.
- **`rename kind a to b`** records that `b` in the target is `a` in the source. The item keeps its prefix, its key
  space and its instances' IRIs; the old name becomes a retired name (section 4.2). A rename is a migration of names
  alone and moves no data.
- **`map T from S1, …, Sn where c { f = e, … }`** makes every live instance of `S1` to `Sn` (each with its subtypes)
  satisfying `c` the origin of one image of `T`. Each field of `T` takes its assignment, else the origin's field of
  the same name and type, else its declared default; a required field with none of the three is refused (`UGS2102`).
  `self` is the origin: assigned alone to a role whose type the origin's image fills, and inside a nested filter the
  way to read the origin's fields rather than the filtered type's.
- **`primary`** marks the map whose images inherit their origins' IRIs. Without it, the map into the type the origin's
  type is renamed to, or into the origin's own type, is primary; any other image is minted afresh (section 4.3).
- **A role's assignment** reaches at most one source instance, the participant's origin, whose image in the role's
  type is the participant: a path from the origin, single-valued by the source's cardinalities, or a lookup of a keyed
  type by its whole key, `Organization(code = self.employer_code)`, which a key makes single-valued. An assignment
  that may reach several instances, or reach a type with no map into the role's type, is refused (`UGS2104`,
  `UGS2107`); one that reaches none for a required role fails the run with the origin named (`UGS2202`).
- **`drop kind q`** declares that the target carries nothing of `q`. A source item the target neither maps, renames
  nor drops is refused (`UGS2101`), so no data is lost by omission.

Several maps may target one type from different source types, as a union; one source type may feed several target
types, as a split. Two maps into one target type from one source type are refused (`UGS2105`).

### 3.3 Example

Version 1 of a namespace `hr` keeps every worker as `Staff` and the employer as an attribute naming an organisation's
code:

```
schema hr version 1
prefix hr = "https://example.org/hr#"

enum StaffKind { employee, contractor }

entity Organization @key(code) { code: String  label: Text }
entity Staff @key(staff_no) {
  staff_no:      String
  name:          Text
  kind:          StaffKind
  employer_code: String Optional
  day_rate:      Decimal(10, 2) Optional
  legacy_ref:    String Optional
}
```

Version 2 splits contractors out, renames the rest, and turns the employer into a relation with its own validity:

```
schema hr version 2
prefix hr = "https://example.org/hr#"

entity Organization @key(code) { code: String  label: Text }
abstract entity Worker @key(staff_no) { staff_no: String  name: Text }
entity Employee   : Worker { }
entity Contractor : Worker { rate: Decimal(10, 2) }
relation Employment {
  worker:   Worker       -> employments*
  employer: Organization -> workforce*
}
```

The migration:

```
migration hr from 1 to 2 {
  rename type Staff to Worker
  map Employee   primary from Staff where kind = employee
  map Contractor primary from Staff where kind = contractor { rate = day_rate }
  map Employment from Staff where employer_code is some {
    worker   = self
    employer = Organization(code = self.employer_code)
  }
  drop attribute Staff.kind
  drop attribute Staff.legacy_ref
}
```

`Worker` keeps `Staff`'s prefix as an abstract type and with it the key space of `staff_no`, named `Staff`. Every
worker keeps its bucket and its IRI `https://example.org/hr#Staff/…`, which is also what the target mints for a worker
written after the cutover, so nothing is stored; `Staff` becomes a retired name of `Worker`. Each `Employee` and
`Contractor` takes a new system ID in its origin's bucket (section 4.5). `day_rate`,
optional in the source and required as `rate` in the target, is required of every contractor origin: one without it
fails the run with the instance named (`UGS2202`). Each `Employment` is a new instance with a fresh handle, anchored
at its worker's bucket, which is its origin's.

## 4. Identity across versions

### 4.1 Key spaces

`SCHEMA.md` section 7.2 derives a keyed instance's bucket and its uniqueness claim from the declaring type's prefix
and the canonical key, and section 7.3 its IRI from the declaring type's name and the key. Under this file all three
read the declaring type's **key space** instead: the prefix and the name of the type that first declared the key. A
type never migrated has its own prefix and name as its key space, so every bucket and IRI minted before a migration is
the one the rules have always given.

- A rename of a declaring type keeps its key space.
- A split whose source type declares the key, as an abstract supertype or as several target types each declaring the
  same key, gives each target the source's key space.
- A union of source types with different key spaces into one keyed target type takes the key space of the first source
  the map names; the others' instances keep their buckets, and their uniqueness claims are registered anew under the
  target's key space during the run (section 6), so one partition again checks each key.
- A key change gives the target a key space of its own, named by the target's declaring type; images keep their
  origins' buckets, and their claims are registered under the new key space.

A key space's name is permanent: a keyed IRI's type segment is the key space's name, never the current name of a
renamed declaring type, so a keyed type mints one IRI form for life.

### 4.2 Names

The catalog keeps, per namespace, every name a type, attribute, role, navigation name or graph has held, mapped to the
item that holds it now or to the version that dropped it. A name is never given to another item of the namespace,
as a prefix is never reassigned (`UGS2109`). A routing step that reads a type name from an IRI resolves retired names
as current ones (section 4.4).

### 4.3 IRIs

An instance's IRI is fixed when it is minted and kept for life, through every migration:

- A primary image inherits its origin's IRI. A keyed image whose key space and key are its origin's, as after a
  rename or a split, mints that same IRI, so nothing is stored. A keyed image under another key space or another key,
  after a union or a key change, stores its birth IRI in the dictionary both ways, as a handle's is (`SCHEMA.md`
  section 7.3). A keyless image keeps its handle IRI, which the dictionary already holds as written.
- A secondary image, and an instance written after the cutover, is minted by the target's rules.
- An origin with no primary image has no IRI in the target; its IRI names it in the source version alone.
- A minted IRI that equals a birth IRI an image holds is refused (`UGS1037`); the two hash to one partition, so the
  check is local.

An IRI's text is the record of its minting, not a statement of the instance's current type or key, which `rdf:type`,
the labels and the key attributes answer.

### 4.4 Routing

Any node finds an instance's bucket from its IRI and the catalog, as `SCHEMA.md` section 7.3 has it: a keyed IRI's type
segment names a key space, whose prefix the catalog gives, and the key read back from the IRI hashes to the bucket; a
handle states its bucket, and its type segment may be a retired name. A primary image stays in its origin's bucket or
leaves a forward entry there (section 8), so an IRI minted before a migration routes to the partition that holds its
instance or forwards to it, one hop, with no lookup. Finding an image by a changed key reads its uniqueness claim, one
hop.

### 4.5 System IDs

An image whose type keeps its origin's type lineage, by identity or rename, keeps the origin's system ID. Any other
image takes a new ID with the origin's bucket, so it lives in the origin's partition. System IDs never leave the
database, so a new ID is invisible to clients; the dictionary's IRI entries are rewritten at the cutover, and a read
at an earlier epoch sees the earlier entries.

## 5. Checks at install

A migration installs with the target's schema source in one Install request (`PROTOCOL.md` section 8.4). The compiler
runs `SCHEMA.md` section 10.1's phases on the target, with phase 8 reading the migration: a destructive change the
migration covers is admitted without the deprecate-and-drop path, and one it does not cover is refused as section 13
there states. The migration's own checks:

| Check | Refused with |
|---|---|
| The namespace is installed at the migration's `from` version, and `to` is the target's version | `UGS2111` |
| Every source item is mapped, renamed or dropped | `UGS2101` |
| Every required target field has an assignment, a same-named source field or a default | `UGS2102` |
| Every assignment's type is its field's | `UGS2103` |
| A single-valued field's or role's path is single-valued by the source's cardinalities; a fold makes a many-valued path single-valued | `UGS2104` |
| One map per target type and source type | `UGS2105` |
| At most one primary map per source type | `UGS2106` |
| A role's assignment reaches a type the role's type has a map from | `UGS2107` |
| A renamed item's new name is free, and no retired name is reused | `UGS2109` |
| A `where` and every assignment are pure and deterministic, as a derivation's (`SCHEMA.md` section 6.8) | `UGS2110` |
| A `map` or `filter` body reads its member alone, and a `map` over a set gives a type a set admits (`SCHEMA-APG.md`, definition 12.4) | `UGS2112` |

The install's reply carries the **migration report**: per target type, its sources, the IRIs kept, stored and minted,
the claims to re-register, the instances that change partition (section 8), and the estimated size of the copy;
whether the mapping is pure or extended, naming each extended term (`SCHEMA-APG.md`, definition 16.3); and the staged
namespace's identifier, which the run is started with.

## 6. The run

A run is a record in the metadata group, which moves through its states by committed transitions, so a run survives
any restart and resumes from its last state.

| State | Entered when | Does |
|---|---|---|
| `staged` | The install succeeds | The target's graphs, prefixes and catalog entries are allocated, invisible to every request but the run's |
| `copying` | The run starts at snapshot epoch `e0` | Each partition computes the images of the origins it owns, reading the source at `e0`, and writes them into the staged graphs; writes to the source continue |
| `validating` | Every partition has copied | The target's rules are checked over the staged data as `SCHEMA.md` section 13.3's scan checks a tightening; a violation fails the run naming the instance (`UGS2202`), a key collision naming both origins (`UGS2201`) |
| `checked` | Validation passes | A **dry run** stops here: the report is final, and the staged namespace is discarded on request |
| `fenced` | The cutover begins at epoch `e1` | Writes to the source namespace are refused as `PROTOCOL.md` must state for a fenced namespace (section 11); reads continue |
| `catching_up` | At `e1` | The images of every origin written in `(e0, e1]`, and of every image whose assignments read one, are recomputed, and the instances they touch validated again |
| `cut_over` | Catch-up passes, at epoch `e2` | One catalog version passes the namespace's name to the target and makes the source read-only; the fence lifts; prepared statements of the namespace go stale (`PROTOCOL.md` section 11.7), and a read-write transaction that executed against the source aborts with Conflict kind Catalog |
| `failed` | Any check fails, or the run is cancelled before `cut_over` | The staged namespace is discarded; the source is as it was, and the fence, if raised, lifts |

The fence is held from `e1` to `e2`, for the catch-up alone, which the report estimates from the source's write rate.
Copying is idempotent per origin: an image is keyed by its origin, so a partition that restarts recomputes what it had
not committed.

## 7. History and reads across versions

A migration carries the live slice: every live origin, and of a `@bitemporal` origin its newest version. An image of
a bitemporal origin keeps the origin's `asserted` start, so "since when was this believed" survives the migration;
the origin's earlier versions and retracted instances stay in the source.

- A read at an epoch before `e2` compiles against the source version and reads the source, as every read compiles
  against the version in effect at its epoch (`SCHEMA.md` section 13.1).
- After `e2`, the source namespace stays installed read-only under the name `{namespace}@{version}`, `hr@1`, with its
  graphs and data, readable by every reader of the namespace and refusing every write. UQL's `believed at` before the
  cutover is answered there.
- A legal purge (`SCHEMA.md` section 8.8) applies to the source's data as to the target's.

## 8. Distribution and cost

An image takes its origin's bucket, so the copy writes in the partition that reads, and the run is one task per
partition. Three cases leave the partition:

- **Reads along a path** that reaches another partition's instances are reads at `e0`, as every read-only read across
  partitions is.
- **Claims registered anew** after a union or a key change are writes at the claim's partition, at most one per image.
- **A relation image whose anchor is not its origin's image**, an `Employment` anchored at a worker that is its origin
  aside, takes its anchor's bucket as `SCHEMA.md` section 9.1 places it; when the anchor's bucket is another
  partition's, the image is written there, and a primary image moved this way leaves in its birth bucket a forward entry
  its IRI routes to, one hop. A migration that changes a relation's `@colocate` is such a case for every image of the
  relation, and the report counts them.

The report states the data copied, the claims registered, the images written elsewhere and the forward entries left,
per partition, so the cost of a migration is known before it runs.

## 9. Clients

A migration rewrites what the database owns and publishes what it cannot rewrite:

- **Catalog functions and derivations** are compiled against the target; one that names a dropped item is refused at
  install (`UGS2101`).
- **Queries** naming a retired name of a renamed item compile against the target for the deprecation window the
  migration sets, with a warning naming the current name. A name retired by a split, a union or a drop is refused
  with the migration named, since it no longer denotes one item.
- **IRIs** clients hold keep naming their instances (section 4.3); no client rewrites a stored IRI.
- **The rename map**, every rename, map and drop of the migration in a machine-readable form, is published with the
  report, for codemods over client code and for the generated API schemas of stage 11, which emit retired names as
  deprecated aliases.

## 10. Refusals

| Number | Condition |
|---|---|
| `UGS1037` | RetiredIri: a minted IRI equals the birth IRI an instance holds |
| `UGS2101` | A source item the target neither maps, renames nor drops, or a catalog item naming a dropped one |
| `UGS2102` | A required target field with no assignment, no same-named source field and no default |
| `UGS2103` | An assignment of another type than its field's |
| `UGS2104` | A path that may yield several instances or values where one is required |
| `UGS2105` | Two maps into one target type from one source type |
| `UGS2106` | Two primary maps from one source type |
| `UGS2107` | A role assignment reaching a type with no map into the role's type |
| `UGS2109` | A rename to a name in use, or a reuse of a retired name |
| `UGS2110` | A `where` or an assignment that is not pure and deterministic |
| `UGS2111` | A migration whose `from` version is not the namespace's installed version, or whose `to` is not the target's |
| `UGS2112` | A `map` or `filter` body that reads more than its member, or a `map` over a set into a type a set does not admit |
| `UGS2201` | Two origins whose images collide on a target key; the message names both |
| `UGS2202` | Migrated data that breaks a target rule; the message names the instance and the rule's own refusal |

## 11. What this specification asks of the others

- **`SCHEMA-APG.md`.** Carried 2026-11-08: the selection and the key lookup, the two steps of section 2 beyond `Δ`,
  and the fragment's semantics (section 16 there).
- **`SCHEMA.md`.** Section 7.2's bucket and claim hash, and section 7.3's keyed IRI, read the declaring type's key space
  (section 4.1 here); section 7.3's routing admits retired names in handle IRIs; section 4.3's birth IRI rule and
  `UGS1037`; section 13.2 points a destructive change at a migration as its alternative to deprecate and drop; section
  10.1's phase 8 reads a migration.
- **`PROTOCOL.md`.** Install carries a migration source and answers the report with the staged namespace's
  identifier; a request starts, stops after `checked`, cuts over, and cancels a run, and one reads its state; a write to
  a fenced namespace is refused with a retryable code of its own; the source's read-only name in requests.
- **`term-dictionary`.** IRI entries rewritten at a cutover, versioned so a read at an earlier epoch sees the earlier
  entry; birth IRIs of images stored both ways; forward entries for moved images.
- **`metadata-group`.** The run's record and its transitions; the retired names per namespace.
- **The query cores.** Retired names compiling with a warning during the deprecation window (section 9).

## 12. Decisions

| Question | Decision |
|---|---|
| Cutover | Decided 2026-11-08: a write fence on the namespace at cutover; no live migration |
| History | Decided 2026-11-08: the live slice migrates; bitemporal history stays read-only in the source version |
| Stage | Decided 2026-11-08: stage 9.5, after conformance, so stage 8's cluster carries a migration that changes placement |
| IRIs | Drafted 2026-11-08: fixed at birth; a keyed IRI names its key space, which outlives renames, and a handle states its bucket, so every IRI routes without a lookup |
| Fragment | Drafted: `Δ` along label and variant-of-label images (`SCHEMA-APG.md`, proposition 9.5), with extended mapping terms, `map` and `filter` over collections, and selections; every image with one origin; no joins, no merging (proposition 16.4 there) |
| Where the mapping is written | Drafted: a `migration` source installed with the target's schema source |
| `asserted` of an image | Drafted: the origin's start, so the time a statement was first believed survives |



## 14. Sources

- D. I. Spivak, *Functorial Data Migration*, arXiv:1009.1166: schemas as categories, instances as functors, Δ, Σ and Π.
- D. I. Spivak, R. Wisnesky, *Relational Foundations for Functorial Data Migration*, arXiv:1212.5303: theorems 3 and 4,
  Σ along a discrete op-fibration and Π compiled to selection, projection, union and key generation.
- P. Schultz, R. Wisnesky, *Algebraic Data Integration*, arXiv:1503.03571: algebraic schemas, uber-flower queries, the
  verification conditions that keep a query's output inside its target's equations.
- `specification/SCHEMA-APG.md`: the algebraic property graph model, schema mappings and `Δ`, proposition 9.5.
- R. Angles et al., *PG-Keys: Keys for Property Graphs*, SIGMOD 2021: the exclusive key a key lookup relies on.
- J. Shinavier, R. Wisnesky, *Algebraic Property Graphs*, arXiv:1909.04881: elements with identity whose values name
  other elements, the model closest to UGS's.
- *Fast Left Kan Extensions Using the Chase*, arXiv:2205.02425: Σ computed by the chase, and its termination limits.
- T. Berners-Lee, *Cool URIs don't change*, W3C, 1998, and *Universal Resource Identifiers: Axioms of Web
  Architecture*, 1996, the opacity axiom: an IRI names, and its text is not read for its meaning.
