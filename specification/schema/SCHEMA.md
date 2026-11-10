# UltraGraph Schema (UGS)

UltraGraph Schema is UltraGraphDB's data definition language. One source declares a hyper-multigraph once, and from it
the server derives everything a schema decides: the catalog the engines check writes and queries against, the type
prefixes and buckets of every system ID, the RDF vocabulary and SHACL shapes SPARQL clients see, the GQL graph type, the
placement of every instance, and the commit path of every write. This file specifies the language and what a conforming
implementation does with it: its lexical structure and grammar, the static rules a schema must satisfy with the
diagnostic each refusal carries, how each construct becomes identity, catalog entries, stored quads and standard views,
how each constraint is enforced at commit, how writes are classified, and how a schema evolves. `../binary_protocol/PROTOCOL.md` beside it
specifies the bytes a schema and its refusals travel in; `SECURITY.md` the privilege a declaration takes. The design and
its reasoning are in `openspec/notes/UltraGraphDB Schema and Query Language.md`; where this file and the note differ,
this file is the specification and section 17 records why.

The language is the version-1 language in full. Section 14 states which of its rules each stage enforces, so a
deployment always knows which declared constraint it checks and which it carries as a declaration only.

## Contents

1. Conventions
2. The data model
3. Lexical structure
4. Grammar
5. Types
6. Declarations
7. Identity: system IDs, keys and IRIs
8. Constraints and their enforcement at commit
9. Placement and write classification
10. Compilation and the catalog
11. Standard views: RDF, SHACL and GQL
12. Storage mapping
13. Schema evolution
14. Staging
15. What this specification asks of the others
16. Worked example
17. Decisions
18. Sources

## 1. Conventions

| Term | Meaning |
|---|---|
| Tenant | The engine's unit of isolation: the u64 of `../binary_protocol/PROTOCOL.md` section 1, holding one catalog of numbered versions in the metadata group (section 11.13 there), one run of buckets, and the grants of its principals. Nothing reads, writes or names across tenants |
| Namespace | A unit of organisation inside a tenant, as a schema is inside a PostgreSQL database: one `schema` source installed in the tenant, with its graphs. Its name is the schema's, and qualifies its types (`temporal.Day`). A tenant holds one or more namespaces; a query may span a tenant's namespaces through imports, whose relations connect them |
| Schema | One `schema` source: a named, versioned set of declarations, which installed becomes a namespace of its tenant. A tenant's catalog version holds its namespaces and the modules extending them |
| Catalog version | The compiled form of every schema and module installed in a tenant, numbered, committed through the metadata group and effective from an epoch |
| Type | A `statement`, `entity` or `relation` declaration. A *type tree* is a type with every descendant; a *declaring type* of an attribute or key is the type whose declaration names it |
| Instance | An entity or a relation instance. Every instance has a system ID whose type prefix is its concrete type's |
| Attribute | A typed value on an entity or relation instance, stored under a predicate |
| Role, participant | A relation connects participants through named roles. The *anchor role* is the role whose participant's bucket the instance takes (section 9.1) |
| Navigation name | The field a participant type gains from a role, with a cardinality bounding how many instances that participant takes part in through the role |
| Partition, home, bucket | As `../binary_protocol/PROTOCOL.md` section 11.11: a partition owns bucket ranges; an entity's home is the partition owning its bucket |
| Epoch | As `../binary_protocol/PROTOCOL.md` section 1. A catalog version takes effect at an epoch |
| Live instance | An instance that is not deleted, and whose `asserted` interval has no end when its type is `@bitemporal` |
| SHALL, refused | A rule the implementation holds. A schema breaking a static rule is refused whole with a diagnostic of section 10.4; a write breaking a commit rule is refused with a refusal of section 8.7, writing nothing of its transaction |

Diagnostics are numbered `UGS0001` to `UGS0999` for compilation (errors, warnings and notes) and `UGS1001` to
`UGS1999` for commit-time refusals; `UGS2001` and up name evolution refusals. A number is never reused.

A rule marked **[stage N]** is enforced from that stage (section 14); before it, the construct is parsed, validated and
recorded in the catalog, and the compilation report names it as declared and unenforced (`UGS0902`).

## 2. The data model

- **Entities** are typed nodes. Entity types form trees by single inheritance; `abstract` types have no direct
  instances. Every entity belongs to exactly one graph.
- **Attributes** are typed values on entities and relation instances. A named attribute type (`attribute Serial =
  String …`) is a reusable declaration of a value type with its annotations.
- **Relations** connect two or more participants through named roles. Every relation instance has identity, so a
  relation declared `@multi` may hold several instances between the same participants, an n-ary relation connects any
  number of roles in one instance, and a relation instance can fill a role in another relation. Relation types form
  trees by single inheritance, with role specialization.
- **Statements** are abstract types that root both entity types and relation types, so a role may accept either.
- **Derived relations** are paths, closures and values computed from an instance's own fields; clients read them and
  never write them.
- **Functions** are named, typed, pure queries stored with the schema.
- **Graphs** are named instances of a schema: an RDF named graph and a GQL graph. Every entity belongs to one graph, and
  a relation may connect instances of any graphs whose schemas it names.
- **Identity everywhere.** Every instance has a 128-bit system ID that never leaves the database, and an IRI by which
  clients name it: minted from its key, or from a handle drawn at its creation when it has none (section 7).
- **Closed by default.** A governed tenant (section 6.1) refuses data its catalog does not declare; `@open` on a
  schema or a type admits undeclared data there, at the cost of classification for it.
- **Invariants are declared, never inferred.** Every constraint is explicit, and the compiler classifies each write
  from the declarations alone.

**Formal semantics.** A catalog's schemas denote an algebraic property graph schema, and a graph's data an algebraic
property graph on it (`SCHEMA-APG.md`): each instance is an element labelled by its concrete type, whose value is a
record of its attributes and roles, so a relation instance has identity and may be referenced by another relation's
role. The standard views of section 11 present that one graph, and the key, presence and single-value rules of
section 8.2 have PG-Keys' semantics.

## 3. Lexical structure

### 3.1 Source

A source is UTF-8 text, at most 16 MiB, with lines of at most 64 KiB, a line's break not counted; a larger source is
refused (`UGS0013`), and a byte sequence that is not UTF-8 at its first invalid byte, one column of length 1
(`UGS0001`). Line
breaks are `\n` or `\r\n`. Whitespace separates tokens and carries no meaning. A compiler reads a source in one pass with
nesting bounded at 64 levels of braces, brackets and parentheses together (`UGS0013`).

### 3.2 Comments

`//` runs to the end of the line; `/*` … `*/` nests. A documentation comment is a run of consecutive lines beginning
with `///`, attached to the declaration, field, role, enum member or graph that follows it; its text, with the marker
and one following space removed per line and lines joined by `\n`, is the item's `doc`. A `///` run followed by anything
but a documentable item is a warning (`UGS0804`) and is dropped.

### 3.3 Identifiers

An identifier is an ASCII letter or `_`, then ASCII letters, digits and `_`, at most 128 bytes. Identifiers are case
sensitive. The convention, checked by the linter as a warning (`UGS0805`) and never by the grammar: types, relations,
statements, enums, records, named attribute types and quantities in `UpperCamelCase`; attributes, roles, navigation
names, functions, schemas, modules, graphs and vocabulary prefixes in `lower_snake_case`; units as their symbol is
written, UCUM's case kept (`m`, `ft_i`, `K`, `dBm`), which the linter leaves unchecked. A qualified
identifier `a.b` names a declaration of an imported schema (`temporal.Day`) or a member of the catalog qualifier
`schema` (`schema.EntityType<T>`).

### 3.4 Reserved words

The following are reserved and never identifiers (`UGS0007`): `abstract`, `as`, `attribute`, `derive`, `entity`,
`enum`, `extends`, `false`, `fn`, `graph`, `module`, `Optional`, `prefix`, `quantity`, `record`, `relation`,
`reserve`, `schema`, `true`.

`statement` is a keyword after `abstract`, the one place the grammar has it, and an identifier everywhere else: a role
or an attribute may be named `statement`, as DeepBrain's provenance links name theirs.

`import`, `unit` and `version` are keywords where a header or a declaration begins, and `version` after a schema's or
module's name; everywhere else they are identifiers, since an identifier never begins a header or a declaration. A
measure's accessor `.unit`, a record field `unit`, an attribute `version` and an enum member `import` are names.

Every other quoted word of the grammar is a contextual keyword, recognised in the position the grammar names and an
identifier everywhere else: the operators `and`, `or`, `not`, `in`, `between`, `is`; the quantifiers `exists`, `some`,
`no`, `all`; the aggregates `count`, `sum`, `min`, `max`, `avg`, `last`, `first`; the modifiers `order`,
`by`, `asc`, `desc`, `limit`; the temporal words `certainly`, `possibly`, `at`, `believed`, `during`, `valid`, `with`,
`retracted`, `open`, `live`; the Allen operators `before`, `after`, `meets`, `met_by`, `overlaps`, `overlapped_by`,
`starts`, `started_by`, `during`, `contains`, `finishes`, `finished_by`, `equals`, `intersects`, `disjoint`,
`within`; and the annotation words `cascade`, `restrict`, `orphan`, `per`, `where`, `type`, `strict`,
`k`, `metric`, `quantization`, `ratio`, `interval`, `ordinal`, `nominal`, `use`. A declaration position, after `:` or
`->`, inside an enum's braces, after `derive`, `fn`, `unit` or `quantity`, holds no operator, so a navigation named
`in`, a role named `by` and an enum member `max` are unambiguous. In an expression, a word that is also an operator is
read as the operator where the grammar expects one and as a name where it expects an operand: `valid contains instant`
compares the field `valid`; `valid at instant` is the shorthand of section 4. A name that differs from a reserved word
by case alone is a name like any other, as the type `Statement` is. Where the grammar expects a keyword and finds an
identifier differing from it by case alone, `optional` after a field's type, the refusal names the keyword as its fix
(`UGS0006`).

The scalar type names of section 5.1 are not reserved words, but a type, enum, record or named attribute type declared
under one of them is refused as a clash with a built-in (`UGS0102`). `schema` is reserved, and its one use as a
qualifier is the catalog's, `schema.EntityType<T>` and the like; `import … as schema` and `prefix schema` are refused
with `UGS0110`, the code naming them. Quantity kinds and
units have name spaces of their own (section 5.8), so a quantity may be named `Time` beside the scalar type.

### 3.5 Literals

| Literal | Form | Examples |
|---|---|---|
| Integer | Decimal digits with optional `_` separators between digits; a leading `-` is the unary operator in an expression | `42`, `50_000_000` |
| Decimal | Digits, `.`, digits; `_` as above | `1_250_000.00` |
| Float | A decimal or integer followed by `e` or `E`, an optional sign and digits; there is no literal for a non-finite number: `inf` and `nan` are identifiers, refused where a number is expected as any identifier naming no value of the type is (`UGS0207`) | `2.5e-7` |
| Rational | Two integer literals around `/`, the second nonzero, exact; accepted as a unit's `multiplier` or `offset`, as a `Decimal` value and in `@range` of a `Decimal` or a `Measure` | `1200 / 3937` |
| String | `"` … `"` with escapes `\"`, `\\`, `\n`, `\t`, `\r`, `\u{XXXX}` (1 to 6 hex digits, a scalar value); any other `\` is refused (`UGS0003`); a string spans no line break | `"[A-Z0-9-]{6,20}"` |
| Boolean | `true`, `false` | |
| Date | `[-]YYYY-MM-DD`, the proleptic Gregorian calendar with astronomical year numbering, the year at least four digits, as XML Schema 1.1 reads it | `2026-11-01` |
| Date-time | A date, `T`, `hh:mm:ss` with an optional fraction of at most 9 digits, then `Z` or `±hh:mm` for a `DateTime`, or no offset for a `LocalDateTime` | `2026-11-01T12:00:00Z` |
| Time | `hh:mm:ss` with an optional fraction and an optional offset | `09:30:00` |
| Year, month | `YYYY` and `YYYY-MM` denote a year and a month at that precision where a temporal value is expected (section 5.5); a four-digit `YYYY` lexes as an integer and is read as a year by the expected type | `2027`, `2026-09` |
| Decade, century, millennium | `YYYYs`, `YYc`, `Ym` where `Y` are the leading digits: `1990s` the decade from 1990, `20c` the century from 2000, `2m` the millennium from 2000 | `1990s` |
| Duration | ISO 8601: `P` with `Y`, `M`, `W`, `D` parts and `T` with `H`, `M`, `S` parts, at least one part, the seconds with an optional fraction | `P30D`, `P1M`, `PT36H` |
| Interval | `a .. b`, `a ..`, `.. b` or `..` with `a`, `b` temporal literals or integers; the whitespace around `..` is optional | `2026-09-14 .. 2027` |
| IRI | `<` … `>`, an absolute IRI as RDF 1.2 writes it, with no whitespace, `<`, `>`, `"`, `{`, `}`, `\|`, `^` or backtick inside | `<https://schema.org/name>` |

A number token is the longest run, from a digit, of digits and `_`, with at most one `.` that a digit follows and an
exponent, `e` or `E` with its optional sign and digits; a run that takes none of the number forms above is refused
whole (`UGS0004`). A number literal in an annotation argument or a default may carry a leading `-`, which is part of
the literal: `@range(-40, 85)`, `= -1`. An integer literal must fit the type it is read into; one that does not is refused where the type
is known (`UGS0207`). A temporal literal that names no calendar instant (`2026-02-30`, `25:00:00`) is refused (`UGS0005`).

### 3.6 Annotations

An annotation is `@` and an identifier, with an optional parenthesised argument list. An argument is a literal, an
identifier, a qualified identifier, a CURIE or an IRI literal (for `@maps_to`), a list `[a, b]`, a named argument
`name: value`, an expression (for `@check` and `@limit`), or a role action `role: cascade | restrict [open] | orphan`. Each annotation names the arguments it takes (section 6); an unknown
annotation is refused (`UGS0008`), a malformed argument likewise (`UGS0009`). An annotation stands on the item its
section allows and nowhere else (`UGS0011`). A type's annotations may stand between its name and its body or after
its body; the two positions form one list. Each annotation appears at most once per item (`UGS0010`), except `@index`
on a field, which may appear once per index kind, `@check`, which may repeat, each a rule with its own message, and
`@maps_to`, which may appear once per external term.

### 3.7 Bounds

| Bound | Limit | Refusal |
|---|---|---|
| A source | 16 MiB; a line 64 KiB; nesting 64 | `UGS0013` |
| An identifier | 128 bytes | `UGS0013` |
| A string literal, its text between the quotes as written; an `@iri` template, a `@pattern` | 4 KiB | `UGS0013` |
| Fields of one type, own and inherited | 4,096 | `UGS0013` |
| Roles of one relation, own and inherited | 64 | `UGS0013` |
| Members of a union, of an enum, components of a key | 32, 4,096, 8 | `UGS0013` |
| Steps of a derivation path, parameters of a function | 32, 32 | `UGS0013` |
| Declared types in a tenant | 65,532, the prefixes 3 to 65,534 | `UGS2005` |
| Sources in a catalog version, schemas and modules together | 1,024 | `UGS0013` |

## 4. Grammar

EBNF: `{ }` zero or more, `[ ]` optional, `|` alternatives, `( )` grouping, terminals quoted. `ident`, `int`,
`rational`, `decimal`, `string`, `iri`, `date`, `datetime`, `time`, `duration` and `interval` are the tokens of section
3. A quoted word that section 3.4 does not reserve is contextual: it is the keyword in the position the grammar names
and an identifier anywhere else.

```
source      = schema | module ;
schema      = "schema" ident "version" int { annot } { header } prefix { header } { decl } ;
module      = "module" ident "version" int "extends" ident ">=" int { annot } { header } prefix { header } { decl } ;
header      = import | prefix | reserve ;
import      = "import" ( string "as" ident | "schema" ident [ ">=" int ] ) ;
prefix      = "prefix" ident "=" string ;
reserve     = "reserve" ident { "," ident } ;
decl        = attribute | enum | record | statement | entity | relation | derive
            | function | quantity | unit | graph ;

attribute   = "attribute" ident "=" type { annot } ;
enum        = "enum" qident { annot } "{" member { "," member } [ "," ] "}" ;   (* qualified in a module alone, section 6.12 *)
member      = ident ;
record      = "record" ident "{" { field } "}" ;
statement   = "abstract" "statement" ident [ ":" qident ] { annot } "{" { field } "}" { annot } ;
entity      = [ "abstract" ] "entity" ident [ ":" qident ] { annot } [ "{" { field } "}" { annot } ] ;
relation    = [ "abstract" ] "relation" ident [ ":" qident ] { annot } "{" { role | field } "}" { annot } ;
field       = ident ":" type [ "Optional" ] [ "=" [ "-" ] literal ] { annot } ;
role        = ident ":" participant [ "as" ident ] "->" ( ident card | "_" ) { annot } ;
participant = union [ "Optional" ] | "[" union "]" ;
union       = ptype { "|" ptype } ;
ptype       = qident | "schema" "." ( "EntityType" | "RelationType" ) "<" qident ">" ;
card        = "?" | "*" | "+" | "{" int "," int "}" ;

derive      = "derive" ident "." ident [ ":" type [ "Optional" ] ] "=" ( dpath [ "exists" ] | expr ) { annot } ;
dpath       = dstep { "." dstep } ;
dstep       = ident [ "(" dfilter { "," dfilter } ")" ] [ "+" | "{" int "," int "}" ] ;
dfilter     = expr | "is" type_or_path | "with" "retracted" | "open" ;

function    = "fn" [ ident "." ] ident "(" [ param { "," param } ] ")"
              "->" type [ "Optional" ] [ card ] "=" expr { modifier } ;
modifier    = "order" "by" qpath [ "asc" | "desc" ] | "limit" int ;
param       = ident ":" type [ "Optional" ] ;

quantity    = "quantity" ident "{" qfield { "," qfield } "}" { annot } ;
unit        = "unit" ident "{" qfield { "," qfield } "}" { annot } ;
qfield      = ident ":" ( literal | ident | "[" ident { "," ident } "]" ) ;
graph       = "graph" ident ":" qident { annot } ;

type        = qident [ "<" targ { "," targ } ">" ] [ "(" int { "," int } ")" ]
            | "schema" "." ( "EntityType" | "RelationType" ) "<" qident ">" ;
targ        = type | int | ident ;
qident      = ident { "." ident } ;
annot       = "@" ident [ "(" [ arg { "," arg } ] ")" ] ;
arg         = ident ":" argval | argval | limit_arg | role_action ;
argval      = [ "-" ] literal | qident | curie | "[" [ argval { "," argval } ] "]" | expr ;
role_action = ident ":" action ;
action      = "cascade" | "restrict" [ "open" ] | "orphan" ;
curie       = ident ":" pn_local ;                   (* pn_local as SPARQL 1.1's PN_LOCAL *)
limit_arg   = aggregate "(" [ ident { "," ident } ] ")" cmp_op literal          (* count() or sum(attr), section 6.7 *)
              "per" ( ident | "(" ident { "," ident } ")" ) [ "where" expr ] ;
literal     = int | rational | decimal | float | string | "true" | "false" | date | datetime | time
            | duration | interval | iri | ident ;                      (* ident: an enum member *)
```

A derive body that is wholly a `dpath`, with or without `exists`, is read as one; any other body is an `expr`, a
computed derivation (section 6.8). The expression grammar, shared by `@check`, `@limit`'s condition, derive
filters, computed derivations and function bodies:

```
expr        = or_expr ;
or_expr     = and_expr { "or" and_expr } ;
and_expr    = not_expr { "and" not_expr } ;
not_expr    = [ "not" ] cmp_expr ;
cmp_expr    = sum_expr [ cmp_op sum_expr
            | "in" sum_expr
            | "between" sum_expr "and" sum_expr
            | "is" ( "some" | "none" | "$" ident | type_or_path )
            | [ "certainly" | "possibly" ] allen sum_expr ] ;
cmp_op      = "=" | "<>" | "<" | "<=" | ">" | ">=" ;
sum_expr    = mul_expr { ( "+" | "-" ) mul_expr } ;
mul_expr    = unary { ( "*" | "/" ) unary } ;
unary       = [ "-" ] primary ;
primary     = literal | "$" ident | "(" expr ")" | "[" [ expr { "," expr } ] "]"
            | "exists" qpath | "some" qpath | "no" qpath | "all" qpath
            | aggregate "(" qpath ")" | qpath ;
type_or_path = ptype | qpath ;   (* a declared type's name is a type test; any other path yields a catalog type *)
aggregate   = "count" | "sum" | "min" | "max" | "avg" | "last" | "first" ;
qpath       = qstep { "." qstep } ;
qstep       = ident [ "(" [ qarg { "," qarg } ] ")" ] [ "+" | "{" int "," int "}" ]
            | "schema" "." ident [ "(" [ qarg { "," qarg } ] ")" ] ;
qarg        = expr | ident ":" expr | "is" type_or_path | "valid" ( "at" | "during" ) expr
            | "with" "retracted" | "open" | "believed" "at" expr
            | "k" ":" int ;
allen       = "before" | "after" | "meets" | "met_by" | "overlaps" | "overlapped_by" | "starts"
            | "started_by" | "during" | "contains" | "finishes" | "finished_by" | "equals"
            | "intersects" | "disjoint" | "within" ;
```

A `qstep` is a field, navigation name, role, function or type name resolved against the expression's context (section
6.7, 6.8); `qstep` with parentheses is a filter list on a navigation or a call of a function, and the filters stand
before the step's bound: `grounds(with retracted){1,32}`. `some`, `no` and `all` quantify a path with its filters: at
least one, none, every element matches, `all` vacuously true over no element; inside the filter, a name resolves against
the element first and the enclosing instance second. A filter item `is T` narrows the element to `T` for the filter
items after it, so a subtype's field may follow it; without it, a field a supertype path does not declare is refused
(`UGS0119`). A bare identifier where an enum value is expected, an operand compared with an enum-typed field among
them, is a member of that enum. The UQL specification extends this grammar for queries; a schema
accepts exactly what is written here, and an expression beyond it is refused (`UGS0120`).

**Evaluation.** The grammar has one evaluation, in a schema and in a query alike:

- **Logic is two-valued.** A comparison with an absent operand is false, arithmetic with an absent operand is absent,
  and `not`, `and` and `or` are Boolean; `is some` and `is none` test presence. So `not (x = 1)` is true where `x` is
  absent.
- **A check of a value holds of a value.** `@pattern`, `@range`, `@values` and a `@check` on an attribute or a record
  field check the attribute's value and hold vacuously of an absent optional one. A `@check` on a type is evaluated
  on the instance as written, so it guards an optional field with `is none or`; one that compares an optional field
  with no test of its presence refuses every instance without the value, and warns at compile time (`UGS0807`).
- **Evaluation fails closed.** A check or a `@limit` condition whose evaluation overflows or divides by zero refuses
  the commit, and so does a computed derivation's (`UGS1039`, the message naming the annotation or the derivation and
  the fault); `UGS1010` stays a check that evaluated and did not hold. No write passes on an expression that could not be evaluated. A division by a literal
  zero is refused when the schema compiles (`UGS0228`).

## 5. Types

### 5.1 Scalar and structured types

Each type names the datatype its values are stored and exported under (`../binary_protocol/PROTOCOL.md` section 10.4 gives the known
datatypes' codes) and its GQL type.

| UGS type | Values | Stored and RDF datatype | GQL type |
|---|---|---|---|
| `Bool` | true, false | `xsd:boolean` | `BOOLEAN` |
| `Int8`, `Int16`, `Int32`, `Int64` | Signed integers of that width | `xsd:byte`, `xsd:short`, `xsd:int`, `xsd:long` | `INT8`, `INT16`, `INT32`, `INT64` |
| `Int128` | Signed 128-bit integer | `xsd:integer`, range-checked at write | `INT128` |
| `UInt8`, `UInt16`, `UInt32`, `UInt64` | Unsigned integers of that width; wider unsigned integers are `Int128` | `xsd:unsignedByte`, `xsd:unsignedShort`, `xsd:unsignedInt`, `xsd:unsignedLong` | `UINT8`, `UINT16`, `UINT32`, `UINT64` |
| `Float32`, `Float64` | IEEE 754 binary32, binary64; `NaN` and the infinities are refused at write (`UGS1011`) | `xsd:float`, `xsd:double` | `FLOAT32`, `FLOAT64` |
| `Decimal(p, s)` | Exact decimal with `p` digits, `s` of them fraction; `1 ≤ p ≤ 38`, `0 ≤ s ≤ p`, `s ≤ 18`, `p − s ≤ 20`, the bounds of the store's i128 × 10¹⁸ encoding (`UGS0203`) | `xsd:decimal` | `DECIMAL(p, s)` |
| `String` | UTF-8 text, compared by code point | `xsd:string` | `STRING` |
| `Text` | Natural-language text with an optional BCP 47 language tag; the one type `@index(fulltext)` takes, so a field wanting both `range` and `fulltext` is `Text` | `rdf:langString` with a tag, `xsd:string` without | `STRING` |
| `Bytes` | Binary data | `xsd:base64Binary` | `BYTES` |
| `Date` | A calendar day | `xsd:date` | `DATE` |
| `Time` | A time of day, with an optional offset | `xsd:time` | `ZONED TIME` |
| `DateTime` | An instant with a UTC offset, compared in UTC | `xsd:dateTimeStamp` | `ZONED DATETIME` |
| `LocalDateTime` | Wall-clock time without an offset; never compared with `DateTime` (`UGS0223`) | `xsd:dateTime` without a timezone | `LOCAL DATETIME` |
| `Duration` | An ISO 8601 duration with a calendar part (months) and an exact part (seconds) | `xsd:duration` | `DURATION` |
| `Interval<T>` | A closed-open interval over `Date`, `DateTime`, `LocalDateTime` or an integer or decimal type, each bound with a precision (section 5.5); the end may be open | Two predicates, `{attribute}_start` and `{attribute}_end`, of `T`'s datatype, and `{attribute}_start_precision`, `{attribute}_end_precision` of `ugdb:precision` where coarser than `T` (section 12.2) | `RECORD<start T, end T, start_precision STRING, end_precision STRING>` |
| `Iri` | An IRI as a value; it creates no entity | `xsd:anyURI` literal | `STRING` |
| `enum Name { a, b }` | One of the declared members | `xsd:string` holding the member's name; `sh:in` lists the members | `STRING` with a value check |
| `List<T>` | An ordered list; `T` a scalar, enum, record, `Iri`, `Measure` or `Interval`; no nested collection (`UGS0205`) | One `rdf:JSON` literal | `LIST<T>` |
| `Set<T>` | An unordered set without duplicates; `T` as for `List` | One triple per member, under the attribute's predicate | `LIST<T>` with no duplicate |
| `record Name { … }` | A named structured value; fields as section 5.4 | One `rdf:JSON` literal | `RECORD<…>` |
| `Vector<F32, n>` | `n` binary32 elements, `1 ≤ n ≤ 65535` (`UGS0204`) | `ugdb:float32vector` | `LIST<FLOAT32>` |
| `Vector<BF16, n>` | `n` bfloat16 elements, `1 ≤ n ≤ 65535` | `ugdb:bfloat16vector` (section 15) | `LIST<FLOAT32>` |
| `Geo` | A GeoJSON geometry in WGS 84: point, line string or polygon | `geo:geoJSONLiteral` | `STRING` holding GeoJSON |
| `Measure<K, u>`, `Measure<K>`, `Measure` | A number with a unit (section 5.8) | `xsd:double`, or `ugdb:measure` when the unit is on the value | `FLOAT64`, or `RECORD<value FLOAT64, unit STRING>` |
| `Unit`, `QuantityKind` | A declared unit or quantity kind as a value | `xsd:string` holding the declared name | `STRING` |
| `schema.EntityType<T>`, `schema.RelationType<T>` | A catalog type at or below `T`, as a value (section 5.10) | The type's class IRI | `STRING` holding the type name |

A type argument or precision outside these rules is refused (`UGS0202`). `Int128` is stored as `xsd:integer` and
checked against its range at write; a larger integer is refused (`UGS1011`).

### 5.2 Optionality, requiredness and defaults

`T Optional` after a field's type makes the value optional; without it the value is required and must be present when
the instance commits (`UGS1005`). A collection attribute, `List` or `Set`, is present when it holds at least one member
and absent otherwise, as the RDF view's one triple per member has it: a required collection means at least one member,
and there is no present-but-empty collection. A field may carry a constant default, `= literal`, whose literal must be
of the field's type (`UGS0207`), with an enum member written bare; a required field with a default takes it when a write
omits the value; a default on an `Optional` field is refused (`UGS0209`), since an absent optional value means absence.
There are no nulls: an absent optional value reads as `null` in the GQL view, as absence in the RDF view and as `None`
in UQL.

### 5.3 Enums and `@values`

An `enum` is a closed set of member names, each unique within it (`UGS0221`). `@open` on an enum lets a module add
members (section 6.12); within one catalog version every enum is closed, so an undeclared member is refused at compile
time (`UGS0112`) and at write (`UGS1030`). `@values(l1, l2, …)` on a field of a scalar type restricts it to the listed
literals, each of the field's type (`UGS0528`). It is a value check of the declaring type, as `@pattern` is, so two
types sharing the attribute may list different values; the stored value stays of the field's type, and each type's
shape exports its list as `sh:in`.

### 5.4 Records, lists and sets

A `record` declares named fields, each `ident: type [Optional]`, with unique names (`UGS0222`); a record field takes
the value checks `@pattern`, `@range`, `@values` and `@check`, checked whenever an instance holding the record commits, and no
other annotation and no default. A record's fields are scalars, enums, `Iri`, `Unit`, `QuantityKind`, `Measure`,
`Interval` or other records, with no cycle through required fields (`UGS0205`). A `List` keeps order and duplicates; a `Set` keeps neither, and a write
holding a duplicate member is accepted as the set of its distinct members. A `Set` of records is refused (`UGS0205`):
set membership compares scalars.

### 5.5 Temporal values, intervals and precision

Temporal values compare only within one type; a comparison or arithmetic between `Date` and `DateTime`, or between
`LocalDateTime` and `DateTime`, is refused at compile time (`UGS0224`, `UGS0223`). The conversions `date(t)`,
`start_of_day(d, zone)` and `datetime(d, time, zone)` are the core functions crossing them. `DateTime − DateTime`,
`Date − Date` and `LocalDateTime − LocalDateTime` give a `Duration`. `Duration` arithmetic adds the calendar part
first, then the exact part. `today()` and `now()` are refused in `@check`, `@limit` and `derive`
(`UGS0225`): they read the epoch's clock and are not deterministic across epochs.

An `Interval<T>` is closed-open, `[start, end)`; `start ≤ end` or the write is refused (`UGS1011`). Either bound may
be open, not both, or the write is refused (`UGS1011`): an open end means until further notice, an open start since
before anything recorded, and an open bound is unbounded in both extents below. An interval without bounds is written
as an absent `Optional` interval. Each bound of a temporal interval carries a **precision**: millennium, century, decade,
year, month, day, hour, minute, second, millisecond, microsecond or nanosecond, no finer than `T`'s; an integer or
decimal interval's bounds are exact. A bound stated at precision `p` stands for the unit it names: `2026-09` is the month
from 2026-09-01 to 2026-10-01. The two extents of an interval `a .. b` with `a` denoting the unit `[a₀, a₁)` and `b` the
unit `[b₀, b₁)`:

| Extent | Start | End | Meaning |
|---|---|---|---|
| Outer | `a₀` | `b₁`, or open | Earliest possible start to latest possible end |
| Inner | `a₁` | `b₀`, or open | Latest possible start to earliest possible end; empty when `a₁ ≥ b₀` |

A bound stated at `T`'s precision has `a₀ = a` and `a₁` one unit of `T` later, so an exact interval's extents are equal.
A lone temporal literal where an interval is expected is the interval of its unit, `a₀ .. a₁`, exact. Every interval
operator comes in three forms: `a certainly R b` holds when `R` holds for every completion of the bounds within their
units, `a possibly R b` when it holds for at least one, and `a R b` means `possibly`. `@index(interval)` stores the
outer extent: a `possibly` form is a range scan over it, and a `certainly` form scans the same candidates and keeps
those the inner extent satisfies, since whatever certainly holds possibly holds.

### 5.6 Vectors

A vector field's dimension is fixed by its type; a write of another dimension is refused (`UGS1028`). `Vector<BF16, n>`
stores bfloat16 and `Vector<F32, n>` binary32. A vector takes no value index, no reverse entry and no dictionary entry
(section 12).

### 5.7 Geometry

`Geo` holds a GeoJSON `Point`, `LineString` or `Polygon` in WGS 84; another GeoJSON object is refused at write
(`UGS1011`). `@index(geo)` builds an S2 cell covering through the store's geo tokenizer: a point's cells, a line string's cover and a
polygon's, so every value the type admits is in its index. A position's uncertainty is an
attribute of its own, a `Measure<Length>`, which a query adds to a search radius.

### 5.8 Quantities, units and measures

```
quantity Length { dimension: "L", scale: ratio, canonical: m } @maps_to(qudt:Length)
unit m          { quantities: [Length], ucum: "m", symbol: "m", multiplier: 1 }
unit ft_i       { quantities: [Length, Altitude], ucum: "[ft_i]", symbol: "ft", multiplier: 0.3048 }
unit dBm        { quantities: [PowerRatio], ucum: "dB[mW]", affine: false }
```

| Declaration | Fields |
|---|---|
| `quantity` | `dimension` (string), `scale` (`ratio`, `interval`, `ordinal`, `nominal`), `canonical` (a unit of this kind, required for `ratio` and `interval`, refused for the others; `UGS0113`) |
| `unit` | `quantities` (one or more quantity kinds), `ucum` (the UCUM code, string), `symbol` (string), and either `multiplier` (1 when absent) with `offset` (0 when absent), the value in the canonical unit being `value × multiplier + offset`, each a decimal or a rational literal, or `affine: false`, which declares a unit that converts only through a function the engine supplies |

A `unit`'s `quantities` must be declared (`UGS0113`); a kind's `canonical` unit must list the kind among its quantities
and is the identity for that kind, so it states no multiplier but 1, no offset but 0 and no `affine: false`
(`UGS0113`). Quantity kinds and units each have a name space of their own: a name is read as a kind in `Measure<K>`,
`Measure<K, u>`'s first argument, `canonical` and `quantities`, and as a unit in `Measure<K, u>`'s second argument and
`canonical`, so a kind or a unit may bear the name of a type, and `UGS0102` applies within each name space.

| Type | Meaning | Stored as |
|---|---|---|
| `Measure<K, u>` | A number of kind `K` in the unit `u`, fixed by the schema; `u` a declared unit of `K` (`UGS0218`) | `xsd:double`; the unit on the predicate's declaration |
| `Measure<K>` | A number of kind `K` in any declared unit of `K`; value and unit stored together | `ugdb:measure`: the lexical form `number␠ucum`, the number in `xsd:double`'s canonical form and `ucum` a declared unit's code |
| `Measure` | A number in any declared unit; its kind is its unit's | `ugdb:measure` |

A write naming an undeclared unit or a unit of another kind is refused (`UGS1027`). Values of one kind of scale `ratio`
or `interval` compare, order and index by their value in the kind's canonical unit, computed on the fly, so no
canonical twin is stored; a unit declared `affine: false` compares within itself only. `convert(q, unit)` is a core
function; version 1 arithmetic is addition and subtraction within one kind and scaling by a number (`UGS0120` for
anything else). Nominal and ordinal kinds take no unit and no arithmetic. Money is a `Decimal` attribute beside a
currency enum, outside `quantity`: currencies convert by rates that change over time, which a unit's fixed multiplier
cannot express. The catalog exposes `schema.quantities` and `schema.units` with their conversions. In an expression a
measure has `.value` and `.unit`, a unit `.quantities`, `.ucum`, `.symbol`, `.multiplier` and `.offset`, and a kind
`.dimension`, `.scale` and `.canonical` (section 5.11). A number literal where a `Measure<K, u>` is expected is a value
in `u`; where a `Measure<K>` or `Measure` is expected, in the kind's canonical unit.

### 5.9 Named attribute types

`attribute Name = type { annot }` declares a reusable value type with its annotations; a field declared with it takes
the type and every field annotation of the declaration, and may add annotations that tighten (`UGS0214` otherwise).
Every attribute value is reachable by value through the store's reverse lists, so "which instances hold this value" is a
lookup for every attribute, and `@index(range)` orders the values. A value that needs relations of its own, a tag in a
hierarchy, is an entity: a `Concept` with a relation to what it tags.

### 5.10 Catalog types as values

`schema.EntityType<T>` accepts entity types at or below `T`; `schema.RelationType<T>` relation types at or below `T`,
with `T` a declared type of the right kind (`UGS0412`). A role may name one (section 6.4), and a parameter or field may
hold one. The value is the type's class IRI; a reference holds the type across versions, so dropping a referenced type
follows the deprecate-then-drop path and is refused while live instances reference it (`UGS2009`). A role naming one
holds a value and takes no existence or liveness check; a type outside `T` is refused as a participant outside the
role's types is (`UGS1018`), and in an attribute as a value outside the attribute's type (`UGS1011`).

### 5.11 Built-in fields

Every instance carries fields the engine answers, which no declaration may take as an attribute, navigation name,
derived name or function (`UGS0404`), nor a relation as a role (`UGS0417`):

| Field | On | Value |
|---|---|---|
| `type` | Every instance | Its concrete type as a catalog-type value, compared with `=` and `<>` and tested with `is` |
| `iri` | Every instance | Its IRI (section 7.3); absent for an edge-encoded relation instance that holds no reifier, which its triple names (section 11.1) |
| `asserted` | A `@bitemporal` type | The transaction-time interval (section 8.4) |
| `live` | A `@bitemporal` type | `asserted.end is none` |
| `open` | A type with a valid interval | `live`, where the type is bitemporal, and the valid interval's end absent |
| `edge` | Inside a navigation step's filter, in a derivation or a function; in a UQL shape inside a binary navigation's shape, as a field with a shape of its own (`../query/QUERY-ULTRA-QL.md` section 7.1) | The relation instance the step traverses, with its attributes, built-in fields and navigations; in a function's `order by`, the instance the body's last step traverses |

A measure has `.value`, the number in its stored unit, and `.unit`; an interval `.start` and `.end`, each `Optional`
where the bound may be open, and `.start_precision` and `.end_precision`; a `Unit` value `.quantities`, `.ucum`,
`.symbol`, `.multiplier` and `.offset`; a `QuantityKind` value `.dimension`, `.scale` and `.canonical`. Each is a
deterministic read of the instance or the catalog and may stand in `@check`, `@limit`, a derivation or a function.

## 6. Declarations

### 6.1 Schema and module headers, vocabulary bases, imports

```
schema aviation version 1
prefix av = "https://example.org/aviation#"
import "https://schema.org/" as sdo
import schema temporal >= 2
reserve Measurement, Reading
```

- `schema ident version int`: the schema's name, which is its namespace's, unique in the tenant (`UGS0102`), and its
  version, a positive integer that rises with each installed revision (`UGS2001`). `@open` on the header admits, in the schema's graphs,
  subjects of no declared type: such a subject is an instance of the built-in `Resource` type, keyed by its IRI, with
  any predicate, each recorded as a default entry (`../binary_protocol/PROTOCOL.md` section 11.13). A `///` run before the header
  documents the schema.
- `prefix ident = string`: the namespace's vocabulary base, an absolute IRI ending in `#` or `/` (`UGS0009`), and the
  prefix SPARQL clients are given for it; exactly one per schema, and one per module for its own base (section 6.12),
  which the grammar requires, so a source without one is refused (`UGS0006`) and a second (`UGS0102`); the prefix is
  never `schema` (`UGS0110`). Every term of the schema is minted under
  the base (section 7.4). Two namespaces in one tenant with one base are refused (`UGS0309`).
- `import string as ident`: an external vocabulary, usable in `@maps_to`; the prefix is unique in the source
  (`UGS0102`) and is never `schema` (`UGS0110`).
- `import schema ident [>= int]`: another namespace of the same tenant, whose types this schema's roles may name
  qualified (`temporal.Day`), so its relations may connect instances of both namespaces; the version bound, when
  given, is checked at install (`UGS0109`). A namespace of another tenant cannot be imported, named or read: a
  tenant is the isolation boundary, and a schema shared by several tenants is installed in each.
- `reserve ident, …`: names no module may declare beneath this schema (`UGS0110`).
- `module ident version int extends ident >= int`: a module extending the named schema or module at least at that
  version (`UGS0109`). Section 6.12 states what a module may declare.

A **governed tenant** is one whose catalog version holds at least one schema. In it, every write names a declared
graph (`UGS1031`), every subject is an instance of a declared concrete type or, under a `@open` schema, a `Resource`,
and every predicate is declared or admitted by `@open` (`UGS1001`). Its catalog is the compilation of its sources and nothing else: a
declaration through the write path (`PROTOCOL.md` section 4.6, kinds 8 to 10) is refused, and so is a Drop of a
predicate a schema declares, which is section 13.2's destructive change. A Drop of everything in the tenant returns
its catalog to the initial one, its namespaces removed (`PROTOCOL.md` sections 6.5 and 11.13). A tenant with no schema keeps the ungoverned
behaviour `../binary_protocol/PROTOCOL.md` section 11.13 states.

### 6.2 Statements

```
abstract statement Statement @key(id) {
  id:          String
  uncertainty: Uncertainty Optional
}
abstract entity   Node     : Statement { }
abstract relation Relation : Statement @multi {
  subject: Statement -> out*
  object:  Statement -> in*
}
```

A `statement` type is always abstract (the grammar requires the word), has attributes and type annotations and no
roles; it may extend another statement type, and an entity type or a relation type may extend a statement type
(`UGS0106` for any other parent kind). Inheritance stays single. A role may name a statement type, so it accepts both
entities and relation instances below it. A navigation name declared on a statement type is gained by every entity and
relation type below it. Entity and relation types share one prefix space, and a statement type covers its descendants'
prefix ranges (section 7.1). A constraint on a statement type applies to every entity and relation subtype.

### 6.3 Entities

```
abstract entity Asset @key(serial) {
  serial:       Serial
  commissioned: Date
  notes:        Text Optional @index(fulltext)
}
entity Aircraft : Asset { tail_number: String @unique }
```

- **Inheritance** is single: `: Parent` names an entity or statement type (`UGS0106`), with no cycle (`UGS0105`). A
  subtype inherits every attribute, navigation name and constraint of its supertype. It may redeclare an inherited
  attribute only to tighten its requiredness and value checks: make an optional attribute required, add a value
  check; an identical redeclaration is allowed and changes nothing. Attribute identity (section 10.2) holds across
  the tree: a redeclaration adding an `@index` or `@immutable` is refused (`UGS0215`), and one leaving out an
  inherited `@immutable` removes it (`UGS0518`); one changing the type or loosening a constraint is refused
  (`UGS0213`, `UGS0214`).
- **`abstract`** types have no direct instances; a write naming one as a concrete type is refused (`UGS1003`).
- **`@key(attr)` or `@key(a, b, …)`** declares the business identity: every named attribute is required, immutable and
  of a keyable type, `Bool`, an integer type, `Decimal(p, s)`, `String`, `Date`, `DateTime`, `Iri`, an enum or `Unit`
  (`UGS0306`, `UGS0301`); a key is unique across the declaring type's tree and per graph; a subtype of a keyed
  type declares no key (`UGS0302`). Entities without a key are allowed and named as section 7.3 states.
- **`@unique`** on an attribute of a scalar, enum or `Iri` type (`UGS0308`) makes its value unique across the declaring
  type's tree, per graph, when present.
- **`@open`** admits undeclared attributes on the type's instances, each recorded as a default entry; those attributes
  take no part in classification.
- **`@iri(template)`**: section 7.3.
- **Types are fixed for life.** An instance's concrete type is its ID's prefix; a write giving an existing keyless
  instance another type is refused (`UGS1004`), and one writing a type on the IRI a keyed type mints is a creation, a
  duplicate of the instance holding the key (`UGS1008`, section 8.1). A change of type is a delete and a create.

### 6.4 Relations

```
relation Supply : Agreement @append_only {
  supplier: Company as party        -> sales*
  buyer:    Company as counterparty -> purchases*
  item:     Asset                   -> contracts*
  value:    Decimal(18, 2) @check(value > 0)
} @limit(sum(value) <= 50_000_000 per buyer)
  @on_delete(supplier: restrict, buyer: restrict, item: restrict)
```

**Roles.** A relation has at least two roles, own or inherited (`UGS0401`). Each role declares a name unique among the
relation's roles and attributes and differing from every built-in field of section 5.11 (`UGS0417`), a participant,
and either a navigation name with a cardinality or `_`.

| Participant form | Meaning |
|---|---|
| `Type` | Exactly one participant per instance |
| `Type Optional` | At most one; the role may be empty |
| `[Type]` | Set-valued: one or more participants per instance; the grammar admits no `Optional` on it |
| `A \| B \| C` | A union: a participant of any member type (section 6.5) |
| `RelationType` | A relation instance, which fills the role through its node's IRI or, edge-encoded, through its reifier's (section 11.1) |
| `schema.EntityType<T>`, `schema.RelationType<T>` | A catalog type as participant (section 5.10); the role declares `-> _`, since no instance type could gain a navigation name (`UGS0413`) |

A participant type is a declared entity, relation or statement type of this schema or an imported one: a name declared
nowhere is an unknown type name (`UGS0201`), and a declaration of another kind is refused (`UGS0402`). Its
participants live in any graph of that type's schema (section 6.11).

**Navigation names and cardinality.** `-> nav card` gives every participant type of the role the field `nav`, unique
among that type's attributes, navigation names, derived names and method functions, inherited ones included
(`UGS0404`). The cardinality bounds how many live instances of the relation one participant may take part in through
this role: `?` at most one, `*` any number, `+` at least one, `{m, n}` from `m` to `n` with `0 ≤ m ≤ n`, `n ≥ 1`
(`UGS0502`). The marker is always written (`UGS0403`). `-> _` declares no navigation name, builds no count index for
the role, adds the instance's partition to no referrer set of the participant (section 9.1), and admits no
cardinality, `restrict` or `@limit` on that side (`UGS0411`): nothing could check them. What a navigation yields:
for a binary relation, the participant at the other role, with the instance's attributes reached as `edge.attr`; for
an n-ary relation, the relation instances, whose other roles are fields.

**`@set` and `@multi`.** `@set`, the default, admits at most one live instance per combination of participants:
inserting an existing combination returns the existing instance, so inserts are idempotent and the instance's
attributes may be updated. `@multi` creates a new instance on every insert. The choice is fixed at the root of a
relation tree (`UGS0409`); a subtype may restate the root's choice, which changes nothing.

**`@symmetric`** applies to a binary relation whose two roles, own or specialized, have one participant type and one
navigation name (`UGS0408`): `(a, b)` and `(b, a)` are one instance. The instance is stored in ID order, the
participant with the lower system ID first: an edge-encoded instance as one triple from it, a node-encoded instance
with its first role, and the super roles the two specialize, filled from it. The navigation reaches the instance from
either participant, and a `@set` combination key takes the participants in ID order.

**Relation attributes** are declared as entity attributes are, and `@key` on a relation gives its instances a business
identity (section 7.3).

**Inheritance and role specialization.**

| Aspect | Rule |
|---|---|
| Inheritance | Single; `: Parent` names a relation or statement type (`UGS0106`); `abstract` relations have no direct instances |
| Inherited roles | A subrelation has every role of its supertype, as declared or specialized once with `as` (`UGS0406`) |
| Specialization | `name: SubType as superrole -> nav card` renames the role, narrows its participant to the super role's type or a subtype of it (`UGS0407`; each member of a union likewise, `UGS0410`), keeps the super role's optionality and set-valuedness (`UGS0407`), and adds a navigation name on the narrowed type; the super role's navigation name still reaches the instance |
| Added roles | A subrelation may add roles; queries over the supertype ignore them |
| Attributes | Inherited; a subtype may tighten, never loosen (`UGS0214`) |
| Navigation names | A supertype's name yields instances of every subtype; a subtype's own names yield its own |
| Cardinality | A bound on a supertype's navigation name counts live instances of all subtypes; a subtype may add a tighter bound on its own name |
| `@set`, `@multi` | Fixed at the root (`UGS0409`) |
| Mutability | A subtype may add `@append_only` or `@immutable`, never remove them (`UGS0518`) |
| Deletes | A subtype may tighten `cascade` to `restrict`, never the reverse (`UGS0519`) |
| `@acyclic` | Declared on a supertype, it applies across all subtypes together |
| Placement | A subrelation may declare its own `@colocate`, over an own or an inherited role; without one it takes the nearest supertype's, and the first role when none declares one. A constraint inherited from a supertype whose state lives at another role is, for that kind, classified as section 9.3 classifies a non-anchor constraint, and the report lists the choice as section 9.4 states (`UGS0904`) |
| Identity | Each relation type has its own prefix; a supertype covers its descendants' ranges |

### 6.5 Union-typed roles

`governor: Rule | Teloid | Mandate as subject -> governs*`: each member type gains the navigation name; navigating to the
role yields a tagged union; a type test against the union is a check against the members' prefix ranges. In a
specialized role, each member must be the super role's type or a subtype of it (`UGS0410`). A union with a repeated or
nested member, one that is a subtype of another member, is refused (`UGS0402`).

### 6.6 Field annotations

| Annotation | On | Arguments | Meaning |
|---|---|---|---|
| `@key`, `@unique` | An attribute | none | Section 6.3 |
| `@immutable` | An attribute | none | Set at creation, never changed; a later write of another value is refused (`UGS1012`). A key attribute is immutable without the annotation |
| `@index(kind, …)` | An attribute, or a materialized derivation (`range` only) | `range`, `fulltext`, `interval`, `geo`, or `ann` with `metric: cosine \| dot \| l2` and optional `quantization: int8 \| none` | Section 6.10 |
| `@pattern(re)` | `String`, `Text`, `Iri` (`UGS0211`) | A regular expression in the syntax of Rust's `regex` crate, anchored at both ends | The whole value matches |
| `@range(min, max)` | An ordered scalar or a `Measure` (`UGS0212`) | Two literals of the type, `min ≤ max`; either may be the identifier `open`; for a `Measure` a number as section 5.8 reads it | `min ≤ value ≤ max` |
| `@values(…)` | A scalar | Literals of the type | Section 5.3 |
| `@check(expr)` | An attribute, a record field, or a type (its instance); may repeat | A deterministic Boolean expression over the instance's own fields and its participants' immutable fields and types, a participant's required, single-valued roles among them, which `UGS1025` fixes (`UGS0226`, `UGS0225`) | Holds at every commit of the instance (`UGS1010`): on an attribute or a record field, of its value when present; on a type, of the instance as written (section 4, *Evaluation*) |
| `@maps_to(curie \| iri)` | An attribute, type, role, navigation name, quantity kind or unit | A CURIE of an imported prefix and a local part as SPARQL 1.1's `PN_LOCAL`, or an IRI literal (`UGS0803`) | Section 11.1 |
| `@iri(string)` | An attribute | An absolute IRI | The attribute's own predicate IRI in place of `{base}{attribute}` (section 10.2) |

An index kind that does not fit the field's type is refused: `range` needs an ordered scalar, `Date`, `Time`,
`DateTime`, `LocalDateTime`, an enum or `Measure` (`UGS0526`); `fulltext` needs `Text` (`UGS0525`);
`interval` needs `Interval` (`UGS0522`); `geo` needs `Geo` (`UGS0523`); `ann` needs `Vector` (`UGS0524`). An index on a
`Set<T>` indexes each member, one entry per member as the RDF view holds one triple per member, and fits the field when
it fits `T`.

### 6.7 Type annotations

| Annotation | On | Arguments | Meaning |
|---|---|---|---|
| `@key(a, …)` | Entity, relation, statement | Attribute names of the type (`UGS0307`) | Section 6.3 |
| `@open` | Schema, entity, relation, enum | none | Sections 6.1, 6.3, 5.3 |
| `@deprecated`, `@deprecated(use: name)` | Any declaration, field, role, enum member or graph | An optional replacement, which the warning names | The item stays enforced and served; a reference to it from another source than the one declaring it warns (`UGS0806`), and from stage 9.5 a reference new in a later version; Describe and introspection mark it; a destructive change drops only a deprecated item (section 13.2) |
| `@abstract` | — | — | Spelled as the keyword `abstract`, never an annotation |
| `@set`, `@multi` | Relation | none | Section 6.4 |
| `@symmetric` | Relation | none | Section 6.4 |
| `@append_only` | Entity, relation, statement | none | Instances are inserted and never updated or deleted (`UGS1013`); section 8.3 |
| `@immutable` | — | — | On attributes only (`UGS0011`) |
| `@bitemporal(valid: attr)` | Entity, relation, statement | An `Interval<DateTime>` or `Interval<Date>` attribute, which may be `Optional` (`UGS0509`) | Section 8.4; implies `@valid(attr)`, and declaring both is refused (`UGS0510`) |
| `@valid(attr)` | Entity, relation, statement | An `Interval` attribute (`UGS0509`) | Names the valid-time interval `valid at` and `valid during` read |
| `@on_delete(role: action, …)` | Relation that is not `@bitemporal` (`UGS0514`) | Each role of the relation at most once (`UGS0515`); action `cascade`, `restrict` or `orphan` | Section 8.5; `orphan` needs an `Optional` single-valued role (`UGS0501`) and no `@append_only` relation (`UGS0601`); `restrict` on a `_` side is refused (`UGS0516`) |
| `@on_retract(role: action, …)` | `@bitemporal` relation (`UGS0513`) | `restrict`, `restrict open` or `cascade` | Section 8.4 |
| `@retracts(role)` | `@bitemporal` relation | A role whose every participant type is `@bitemporal` (`UGS0512`) | Inserting an instance retracts its participant in that role in the same commit |
| `@ends(role)` | Relation with a valid interval | A role whose every participant type has a valid interval (`UGS0512`) | Inserting an instance closes its participant's valid interval at the instance's own valid start, in the same commit; section 8.4 |
| `@acyclic` | Binary relation, or an abstract relation whose subtypes are binary, whose two roles' participant types share a common supertype (`UGS0503`) | Optional `open` and `max_depth: n`, `1 ≤ n ≤ 4096` (`UGS0504`) | No cycle through the relation's two roles, across the type tree; `max_depth` bounds every path's length, counted in relation instances, and a check visits at most `rule_read_max` relation instances whether or not it is given (section 8.2); `open` counts only live instances whose valid interval is open (needs `@valid`, `UGS0508`) |
| `@colocate(role)` | Relation without `@key` (`UGS0702`) | A role of the relation, own or inherited (`UGS0701`) | Section 9.1 |
| `@limit(agg op n per (roles [, type]) [where cond])` | Relation | `agg` is `count()` or `sum(attr)` over a numeric attribute, any other aggregate the grammar admits refused at phase 5 (`UGS0505`); `op` is `<=`, `>=` or `=`; `n` a literal; `per` names one or more roles, optionally the word `type`, and one role alone under a lower bound (`UGS0529`); `cond` a deterministic expression over the instance's own values, its participants' types and `open` (`UGS0507`); a top-level conjunct over the `per` participants' types selects the groups | Section 8.6 |
| `@no_overlap(per: [roles])`, `@no_overlap(per: [roles], strict)` | Relation or entity with `@valid` or `@bitemporal` (`UGS0508`) | Roles of the relation | The valid intervals of live instances sharing those participants never certainly overlap, or with `strict` never possibly overlap (`UGS1024`) |
| `@access` | Relation | none | An access-control relation: a query filtering through it runs at snapshot level, never monotonic-fast |
| `@iri(template)` | Keyed entity or relation | A string template (section 7.3) | Overrides the instance IRI pattern |
| `@maps_to(curie \| iri)` | Any type | As on a field | Section 11.1 |
| `@materialized`, `@virtual` | `derive` | none | Section 6.8 |
| `@default`, `@base(string)` | `graph` | | Section 6.11 |

Where a rule above says a type is `@bitemporal`, a subtype of a `@bitemporal` type counts.

### 6.8 Derived relations

```
derive Asset.all_parts   = parts+
derive Asset.near_parts  = parts{1,3}
derive Company.suppliers = purchases.supplier
derive Supply.certified  = certification exists
```

`derive Type.name = body`: `Type` is an entity, relation or statement type of this schema and `name` is unique on it as
a navigation name is (`UGS0404`). The body is a path of navigation names, roles, derived names and method functions
from `Type`, each step with optional filters, a transitive marker `+` (one or more steps, distinct endpoints) or a bound
`{m, n}`, and `exists` turning the path into a Boolean; or an expression over the instance's own fields, a computed
derivation (below). The result type and cardinality follow from the last step: `+` and `{m, n}` yield a set of distinct
endpoints; a declared `: type` must equal the inferred one (`UGS0207`). An aggregate over a path is a function's
(section 6.9), computed when read. A
derivation's filters use the expression grammar with the bitemporal items `with retracted` and `open`; `today()` and
`now()` are refused (`UGS0225`).

- **Read-only.** A client write naming a derived name is refused (`UGS1029`).
- **`@materialized`** stores the result and maintains it in the same local commit as the write that changes it; it is
  allowed only when every step of the derivation stays within the anchor partition of the deriving instance: a computed
  derivation, and a path over relations colocated with `Type`'s role (`UGS0704` otherwise, naming the step that leaves
  the partition). It is the default for computed derivations. **[stage 6]**
- **`@virtual`**, the default otherwise, expands the derivation at query time. A derivation whose filter reads another
  instance's state, a ground's `live` or `rank`, is virtual; where the maintenance matters, declare it on the type that
  holds the state read.
- A step's filters precede its bound, `parents(open){0,32}`; `{0, n}` includes the start instance itself, and `+` and
  `{m, n}` with `m ≥ 1` do not.
- Closures over relations that may contain cycles use set semantics, so they terminate.
- `@index(range)` may follow a materialized derivation of a scalar type.

**Computed derivations.** `derive Asserted.closed: Bool = not live or valid.end is some or rank = deprecated` names a
value computed from the instance alone: its body is an expression over the instance's own attributes and the built-in
fields of section 5.11, with no navigation, role, derived name of another instance or function reading one
(`UGS0227`), and `today()` and `now()` refused (`UGS0225`). Its type is declared and equals the expression's
(`UGS0207`). It is materialized by default and maintained in every commit that writes the instance, its retraction
included, so it lives in the instance's partition by construction; `@index(range)` may follow it, and a derivation or
a filter elsewhere reads it as a field. **[stage 6]**

### 6.9 Functions

```
fn Aircraft.engines() -> Engine* = all_parts(is Engine)
fn Company.spend(since: Date) -> Decimal(18, 2) = sum(purchases(signed >= since).value)
fn certified_suppliers() -> Company* = Company(some sales(exists certification))
```

A method function `Type.name(params) -> type card = expr` is used like a navigation name; a free function
`name(params) -> type card = expr` is a root selector. Parameters and the return type are declared; the body is checked
against them (`UGS0207`), with a parameter named in the body resolved before any field of the same name. Bodies are
pure and deterministic (`UGS0225`), and the call graph of all functions in a catalog version is acyclic (`UGS0118`).
Calls are inlined into the calling query's plan; a function is monotonic exactly when its body is. A body may end with
`order by` on a path of its result, `asc` or `desc`, and `limit`; inside a step's filter `edge` is the instance the
step traverses (section 5.11). A function's name is unique as a navigation name is (`UGS0404`, method) or among free
functions and types (`UGS0102`). Functions are versioned with the schema and exposed through introspection; the
standard views do not expose them.

### 6.10 Indexes

| Index | Built from | Store index |
|---|---|---|
| `@index(range)` | An ordered field | The datatype's sortable tokenizer: `int`, `float`, `bool`, `hour` for `DateTime`, `LocalDateTime` and `Time`, `day` for `Date`, `exact` for `String`, `Text`, enums, `Iri` and `Unit`, `float` for `Measure<K, u>`, and a `measure` tokenizer over the canonical value for `Measure<K>` and `Measure` (section 15); a range filter scans the tokens and compares the values |
| `@index(fulltext)` | `Text` | The `fulltext` tokenizer, analyzer by language tag |
| `@index(interval)` | `Interval` | Range indexes on the outer extent's two bounds (section 5.5) |
| `@index(geo)` | `Geo` | The `geo` tokenizer |
| `@index(ann, metric: m, quantization: q)` | `Vector` | The `hnsw` factory with `metric` `cosine`, `dotproduct` or `euclidean` for `cosine`, `dot`, `l2`; `quantization: int8` quantizes the index and rescores the top candidates in the stored precision **[stage 8]**. The factory's build parameters take the store's defaults; a schema naming a search-time parameter such as `ef_search` is refused (`UGS0524`), since it belongs to the query |
| implicit | Every `@unique` attribute and every single-attribute `@key` | The predicate's `unique` flag; lookup by value through the reverse list. A composite key's claim is on the key as a whole, and its components' predicates carry no `unique` flag |
| implicit | Every navigation name with a bounded cardinality, `?`, `+`, `{m, n}` | A count index on the role's predicate |

Indexes are derived data written in the instance's partition in the same local commit as the write (`../binary_protocol/PROTOCOL.md`
section 11.13 for the vector index, which the indexer maintains in commit order outside the transaction). An index added
to a field holding data is built in the background from the data at the version's epoch, as `PROTOCOL.md` section 11.13
states.

### 6.11 Graphs

```
graph ontology : deep_brain @default
graph archive  : deep_brain @base("https://example.org/archive/")
```

`graph ident : schema { annot }` declares a named graph over a schema of this tenant (`UGS0116`). Its IRI is
`{base}{ident}`, with `base` the schema's vocabulary base. The graph's **instance base**, under which its instances'
IRIs are minted (section 7.3), is the schema's vocabulary base, or the IRI `@base(string)` names; two graphs of one
schema take two instance bases (`UGS0309`). At most one graph in a tenant carries `@default` (`UGS0102`); a write naming
the default graph lands in it, and a governed tenant without one refuses writes to the default graph (`UGS1031`).

Every entity belongs to exactly one graph, the graph of the write that creates it, which must be a graph of its type's
schema (`UGS1031`). A relation instance belongs to the graph of the write that creates it, a graph of the relation's
schema, and each participant lives in a graph of its own type's schema, the relation's or one it imports (section 6.1),
so one relation connects instances of several graphs and namespaces and is placed and checked as any relation is
(section 9). A reverse navigation from a participant in another graph reads through the participant's referrer set
(`../binary_protocol/PROTOCOL.md` section 11.11).

### 6.12 Modules

```
module sr version 3 extends deep_brain >= 5
prefix sr = "https://example.org/sr#"
```

A module extends a schema, or another module, at least at the named version, and implicitly imports it, so the
extended schema's types are named qualified or bare. It may declare: entity and relation types
beneath any type of what it extends (`: deep_brain.Node`, qualified or bare when unambiguous), named
attribute types, enums, records, `quantity` and `unit` declarations, members of enums the core declares `@open`
(`enum Rank { ... }` in the module adding members to `deep_brain.Rank` is written `enum deep_brain.Rank { new_member }`),
derivations and functions on its own types, and graphs over itself. An enum's qualified name is a module's extension
of an enum of what it extends; in a schema, which extends nothing, a qualified enum name is refused (`UGS0006`). It declares no key on a keyed type's subtype, no
new root type (`UGS0106`: a module's entity or relation type names a parent), and tightens inherited constraints only
(`UGS0214`). Two modules may not declare one name, and a module may not declare a name the core reserves (`UGS0110`,
`UGS0111`). A module's terms are minted under its own vocabulary base. A module's installation is an additive change
of the tenant's catalog version; a module upgrade is a version like any other. `@open` enums are closed within each
catalog version.

## 7. Identity: system IDs, keys and IRIs

### 7.1 System IDs and type prefixes

Every instance has a 128-bit system ID as `../binary_protocol/PROTOCOL.md` section 1 and the `system-identity` specification lay it out:
from the most significant bit, a 16-bit type prefix, a 64-bit sequence unique per type and never reused, a 24-bit
bucket and 24 reserved zero bits, compared as big-endian. No ID leaves the database.

- Prefix 0 is invalid; 1 is the built-in `Resource` type, 2 the built-in `Statement` type for statements of ungoverned
  or `@open` data. The metadata group allocates each declared entity, relation and statement type a prefix from 3 in
  **declaration order**: the first catalog version holding a type assigns the next free prefix to each new type in the order of the types' declarations in the installed sources, an order the sources alone decide: schemas by
  name as UTF-8 bytes, then modules, each after what it extends and ties by name, each source in text order; a later version assigns the next free prefixes to the types it adds. A prefix is never reassigned,
  renumbered or reused, even after its type is dropped. A tenant whose free prefixes run out refuses the version
  (`UGS2005`): 65,532 declared types, the prefixes 3 to 65,534. Prefix 65,535 is never allocated, so the next free
  prefix and a prefix range's end fit 16 bits (`CATALOG.md` sections 2 and 5.3).
- A type's **prefix set** is its own prefix and its descendants'. The catalog computes it per version as sorted
  half-open ranges, one when the tree was declared together, more once later versions or modules added subtypes. A
  type test is a check against those ranges; a supertype scan is one range scan per range. Abstract types hold a prefix,
  with no instances.
- **Buckets.** A keyed entity's bucket is `Bucket::derive(tenant, prefix of the key's key space, canonical key)`
  (section 7.2). A keyless entity takes the bucket its handle states (section 7.3). A keyed relation
  instance takes the bucket its key derives, as a keyed entity does, and so declares no `@colocate`. An unkeyed relation
  instance takes the bucket of its anchor participant (section 9.1).
- **Sequences** are leased per (type, partition) by the metadata group (`metadata-group` specification).
- **Declared predicates.** In a governed tenant each predicate a catalog version declares takes the `Resource` prefix
  1, its IRI's bucket, and a sequence the metadata group allocates with the version from 2^63 upward, to the version's
  new predicates in the order of their Predicate entries, never reissued; the `Resource` sequences it leases to
  partitions stay below 2^63. The version holds each declared predicate's sequence (`CATALOG.md` section 2), so every
  role reads a declared predicate's ID from the version in effect and no dictionary entry is made for it.

### 7.2 Keys and their canonical encoding

The canonical encoding of a key is, over the key's attributes in the order `@key` lists them, the concatenation of each
component's **4-byte big-endian length** and its bytes, where the bytes are the component's canonical lexical form in
UTF-8: for `String` and `Iri` the text itself; for `Bool`, the integer types, `Decimal`, `Date` and `DateTime`
the canonical form `../binary_protocol/PROTOCOL.md` section 10.5 gives, a `DateTime` normalised to UTC first; for an enum the member's
name; for `Unit` the unit's declared name. A `Resource`'s key is its IRI as one component, which is the encoding the
`system-identity` specification already fixes, so governed and ungoverned data hash alike. The encoding is part of the
identity contract: a change to it is a new major version of the mapping contract.

**Key spaces.** A key's **key space** is the prefix and the name of the type that first declared the key. A migration
carries it through every rename and split of the declaring type, and gives a key that changes, or a type a union
creates, a key space of its own (`SCHEMA-EVOLUTION.md` section 4.1). A type no migration has touched has its own prefix
and name as its key space, so the rules below give every instance written before a migration the bucket, claim and
IRI they always gave it.

The bucket's low 16 bits are the top 16 bits of XXH3-64, seed 0, of the key space's prefix as 2 big-endian bytes
followed by the canonical encoding; its top 8 bits are the tenant's (`system-identity`). The key space's prefix, not
the concrete type's, keeps every instance of a key's tree in the partition that checks the key's uniqueness.

A `@key`'s uniqueness claim is kept in the entity's own bucket: the claim's hash input is the key space's prefix and
the canonical key, the entity's own, so creating a keyed instance and checking its key meet in one partition. An
instance a migration keeps in its origin's bucket under another key space has its claim registered under the new key
space, in the partition that key hashes to (`SCHEMA-EVOLUTION.md` section 4.1). A
`@unique` value's claim is at `Bucket::of_value(tenant, predicate, value)`.

A key is immutable (`UGS1007`) and required (`UGS1006`); a second instance of the tree with the same key in one graph
is refused (`UGS1008`) while the first is live, and for good once the first is of a `@bitemporal` or `@append_only`
type. **Claims of history-keeping types are permanent:** a retraction releases neither the key's claim nor a `@unique`
value's, so a retracted instance keeps its key, its values and its IRI, which `believed at` and `with retracted` reads
still resolve, and no later instance is given them. What a retracted statement asserted is asserted again by an instance
under another key, which may name the retracted one, as `Supersedes` does. A deleted instance of another type releases
its claims (section 8.5).

### 7.3 Instance IRIs

| Instance | IRI |
|---|---|
| A keyed entity | `{base}{KeySpace}/{key}`: the instance base of the entity's graph, the name of the key's key space (section 7.2), which is the declaring type's name unless a migration renamed that type, `/`, and the key's components, each percent-encoded so that every byte outside RFC 3986's unreserved set (`ALPHA`, `DIGIT`, `-`, `.`, `_`, `~`) is encoded, in uppercase hexadecimal digits, joined by `/` for a composite key. The key space is named so a subtype's instances share their supertype's IRI space, a renamed type keeps its IRIs, and the key reads back from the IRI |
| A relation instance with `@key` | `{base}{KeySpace}/{key}` likewise |
| A keyed instance of a type with `@iri(template)` | The template, an absolute IRI with `{attr}` placeholders for every key attribute, each component percent-encoded as above, and literal text between them. Because a component holds only unreserved characters after encoding, a separator holding at least one other character never occurs inside a component, and the key reads back by splitting on the literal text. The compiler refuses a template that omits a key attribute, that names an attribute twice or one that is not a key attribute, that puts two placeholders with no literal text between them or with text of unreserved characters only, or that is not absolute (`UGS0304`); and two templates in a tenant one of which is a prefix of the other, as text with placeholders removed (`UGS0305`), so an IRI resolves to one type |
| A keyless entity, and a relation instance without `@key`, an edge-encoded one through its reifier | `{base}{Type}/{handle}`: the instance base of its graph, its concrete type's name, `/`, and its **handle**: the bucket in 6 and 104 random bits in 26 lowercase hexadecimal digits, joined by `-`, drawn at creation as the `term-dictionary` specification draws a skolem IRI's, the bucket from those the creating partition owns in the tenant's run and the bits uniformly, drawn again on a collision. A type is keyed or keyless whole, so key IRIs and handle IRIs never share a type's space. The dictionary holds the IRI and the ID both ways, as it holds a reifier's skolem IRI; the RDF view presents it as an IRI |

**Subjects at creation.** An instance is created under the IRI its type mints for it and under no other: a keyed
instance under the IRI its key attributes mint, by the pattern above or by its template, and a keyless one under a
blank node, which its partition replaces by a handle. A write creating an instance under another subject, another IRI
or, for a keyed type, a blank node, is refused (`UGS1038`): every node finds an instance's bucket from its IRI and the catalog, so an IRI the key does not
mint would route to a bucket the key does not hash to, and one write would have two spellings. The key is never read
out of the IRI in place of the attributes, which the write states (`UGS1006`). A key attribute written with another
value on an existing instance is `UGS1007`.

An instance a migration creates as a secondary image of an origin (`SCHEMA-EVOLUTION.md` section 4.3) takes, when its
type is keyless, a **derived handle** in place of a drawn one: the origin's bucket and the low 104 bits of XXH3-128,
seed 0, of the origin's IRI, the target type's name and, for an image of an unnested member, the member's canonical
encoding (section 7.2) and a list member's position. Evaluating the migration again, as a read of the target's
history does, names the image alike.

**Birth IRIs.** An instance's IRI is fixed when it is minted and kept for life, through every migration
(`SCHEMA-EVOLUTION.md` section 4.3). A migration image under another key space or another key than its origin's keeps
its origin's IRI as its **birth IRI**, held in the dictionary both ways as a handle is. A minted IRI equal to a birth
IRI an instance holds is refused (`UGS1037`); both hash to one partition, so the check is local. An IRI's text records
its minting; `rdf:type`, the labels and the key attributes state the instance's current type and key.

An ungoverned blank node keeps the term dictionary's skolem IRI under `.well-known/genid`, presented as a blank node.

Any node finds an instance's bucket from its IRI and the catalog: for a keyed instance the template names the key space,
by its name or a name it held before a migration (section 10.2's retired names), the key reads back and section 7.2
gives the hash; for a handle its first six digits are the bucket, and its type segment may be a retired name. System IDs
never appear in IRIs; a handle shows a bucket and nothing else of the ID.

### 7.4 Vocabulary IRIs

| Term | IRI |
|---|---|
| A type | `{base}{Type}` |
| An attribute | `{base}{attribute}`, or the IRI its `@iri` names: one predicate per attribute name per vocabulary base, or per `@iri` (section 10.2) |
| A navigation name | `{base}{nav}` |
| A role of a node-encoded relation | `{base}{Relation}_{role}` |
| An enum | `{base}{Enum}`; a member, `{base}{Enum}_{member}` for documentation; the stored value is the member's name |
| A derived name | `{base}{name}` |
| A graph | `{base}{graph}` |
| An interval bound | `{base}{attribute}_start`, `{base}{attribute}_end`, `{base}{attribute}_start_precision`, `{base}{attribute}_end_precision` |
| A built-in field of section 5.11 in the view | `ugdb:asserted_start`, `ugdb:asserted_end`, `ugdb:live`, one predicate each for every schema |

Two terms minting one IRI are refused (`UGS0801`), as are two terms differing only in a way the GQL view cannot tell
apart (`UGS0802`: a label and a relation node type of one name). Names are compared byte for byte, the GQL core's identifiers
being case-sensitive, so `precedes` and `Precedes` are two names.

A derived name's and a navigation name's IRI is a declared predicate, so a client's write of one the view derives is
refused as a derived triple (`UGS1029`), never as an undeclared predicate (`UGS1001`).

## 8. Constraints and their enforcement at commit

### 8.1 Instance assembly and whole-instance validation

Every write, in UQL, GQL or SPARQL, reaches the partition as changes to quads, and a delete or a retraction of an
instance as the operation naming it (`../binary_protocol/PROTOCOL.md` section 7.3). The partition assembles the changes of a
transaction into instances before it validates them: the changes sharing a subject form the writes to that instance,
a subject being an entity's IRI, a node-encoded relation instance's handle IRI, or an edge-encoded instance's triple with
its reifier. Each instance a transaction creates or changes is validated whole, against the catalog version in effect at
the commit's epoch (`../binary_protocol/PROTOCOL.md` section 11.13), when the transaction commits; a violation refuses the whole
transaction with the refusal naming the instance and the rule, and nothing of it is held. A `@set` relation's insert of
an existing combination is resolved to the existing instance before validation.

A transaction **creates** an instance when it writes `rdf:type` on a blank node or on the IRI a keyed type mints. A
blank node's label names one new instance within one request (`../binary_protocol/PROTOCOL.md` section 4.4), so an instance created
under one is created whole in that request, and a later request names it by its handle. A creation whose key an
instance of the tree already holds is a duplicate (`UGS1008`), whatever concrete type it names, and is never read as a
write to that instance: instances are not merged. Two concrete types in one creation, and a type written on an
existing keyless instance, are `UGS1004`.

A commit reports one refusal: the first rule broken in the order of section 8.2's table, which takes a value's type
before any check that reads it and the more specific of two rules first, for the first instance in the order its
changes arrived.

A created entity must carry its concrete type: an `rdf:type` triple to one concrete declared type of its graph's schema,
or the type the UQL or GQL statement names (`UGS1002` for none or an unknown one, `UGS1003` for an abstract one,
`UGS1004` for two), under the subject its type mints (section 7.3; `UGS1038`); a blank node given attributes and no
`rdf:type` creates an instance naming none (`UGS1002`). A created relation instance must carry every required,
single-valued role filled (`UGS1015`) and every set-valued role non-empty (`UGS1016`), each participant of the role's
type (`UGS1018`), a participant being an entity, a node-encoded instance or an edge-encoded instance named by its
reifier, and its required attributes (`UGS1005`). A reifier a client writes carries its relation's concrete type, an
`rdf:type` triple, and an edge-encoded instance of a `@bitemporal` or `@multi` relation is written as its triple with
its reifier. A reifier written without its triple or its type, such an instance's triple written without its reifier,
a role triple without its instance's type, or a triple of a node-encoded relation outside its instance, is an
incomplete instance (`UGS1034`).

### 8.2 The constraints

What each counts: live instances, unless the row says otherwise. Where it is checked: section 9.3.

| Constraint | Checked | Refusal |
|---|---|---|
| Instance complete: a reifier with its triple and its type, a `@bitemporal` or `@multi` edge-encoded instance's triple with its reifier, a role triple with its instance's type, a node-encoded relation's triple inside its instance (section 8.1) | At every write | `UGS1034` |
| Concrete type declared, in its graph's schema | At creation | `UGS1002`, `UGS1003`, `UGS1031` |
| Type fixed for life: one concrete type in a creation, none written on an existing keyless instance (section 8.1) | At every write of `rdf:type` | `UGS1004` |
| A retraction names an instance of a `@bitemporal` type | At a retraction | `UGS1026` |
| Declared predicate, or `@open` | At every write | `UGS1001` |
| Derived name, or a triple the view derives (an inverse navigation, a node-encoded relation's navigation property, a super role's triple), written or deleted by a client | At every write | `UGS1029` |
| One value of a single-valued attribute | At every write of the attribute | `UGS1036` |
| Key present | At creation | `UGS1006` |
| Required attribute present; default applied | At creation and at a removal of a value | `UGS1005` |
| Datatype, enum member, range of the integer type, vector dimension, unit declared and of the kind, interval ordered and bounded | At every write of the value | `UGS1011`, `UGS1030`, `UGS1028`, `UGS1027`, `UGS1011` |
| Subject the IRI its type mints: by key or template for a keyed type, a blank node for a keyless one (section 7.3) | At creation | `UGS1038` |
| Minted IRI no instance's birth IRI (section 7.3) | At creation | `UGS1037` |
| Key immutable, unique across the tree per graph, retracted instances of history-keeping types included (section 7.2) | At any write of a key attribute; at creation, uniqueness decided at the value's claim | `UGS1007`, `UGS1008` |
| `@unique`, retracted instances of history-keeping types included | At a write of the attribute; decided at the value's claim | `UGS1009` |
| `@pattern`, `@range`, `@values`, `@check` | At every commit of an instance whose checked values changed | `UGS1010` |
| `@immutable` | At a write replacing a present value | `UGS1012` |
| Participant exists, is live, is of the role's type | At creation of the instance and at a change of its participants, which only an `Optional` or set-valued role admits | `UGS1017`, `UGS1019`, `UGS1018` |
| Role present: a required, single-valued role filled; a set-valued role non-empty | At creation and after a removal of a participant | `UGS1015`, `UGS1016` |
| A required, single-valued role's participant fixed for the instance's life | At a write replacing the participant | `UGS1025` |
| Participation cardinality `?`, `+`, `{m, n}` | At an insert, delete or retraction changing the participant's count through the role; a lower bound is checked at the commit that creates the participant or removes an instance, so a participant and its first instance are created in one transaction | `UGS1014` |
| `@set` | At insert: the combination resolves to the existing instance | none; the insert is idempotent |
| `@acyclic`, `max_depth` | At an insert or a participant change of the relation; under `open`, at an update of the valid interval | `UGS1021`, `UGS1022`, `UGS1040` |
| `@limit` | At an insert; at an update of the aggregated attribute or of an attribute its condition reads; at a delete or retraction; and for a lower bound at the creation of the grouped participant | `UGS1023` |
| `@no_overlap` | At an insert or an update of the valid interval or of a `per` participant | `UGS1024`, `UGS1040` |
| `@append_only` | At an update or delete of the instance | `UGS1013` |
| `@bitemporal` revision: an update opens a new version; participants retracted | At an update; at an insert naming a retracted participant | `UGS1019` |
| `@ends(role)` | At the insert | `UGS1035` |
| `@on_delete(restrict)`, `@on_retract(restrict)`, `@on_retract(restrict open)` | At the participant's delete or retraction | `UGS1020` |
| Every expression of the schema evaluates for the instance | At every commit evaluating it | `UGS1039` |
| `Set` duplicates | At write: collapsed | none |

The key, uniqueness, presence and single-value rows have PG-Keys' semantics (Angles et al., *PG-Keys*, SIGMOD 2021;
`SCHEMA-APG.md` section 10). A `@key` is an `IDENTIFIER` key, exclusive, mandatory and singleton, whose scope is the
key's type tree in one graph; `@unique` is an `EXCLUSIVE` key; a required attribute or role is `MANDATORY`, and a
single-valued one `SINGLETON`. Each holds when its violation query returns nothing. Participation cardinality takes the
bounded `COUNT` form PG-Schema proposes. `@acyclic`, `max_depth`, `@limit`, `@no_overlap` and the bitemporal rules are
UGS's own.

**What a rule reads beyond the instance is bounded.** Cardinality reads its count index and `@limit` its group's
aggregate (section 8.6): one read each, whatever the group's size. `@acyclic` walks the relation's paths from the
instance and `@no_overlap` reads the live instances sharing its participants; each check visits at most
`rule_read_max` relation instances, 4,096, a bound of this specification, counting each it visits once: the walk's for
`@acyclic`, the group's live instances for `@no_overlap`. A check that would visit more is refused before it does, with
`UGS1040`, naming the rule and the bound, and is never decided past it, so a commit's latency is the schema's to bound
and never the data's. `max_depth` counts the relation instances along a path, so a fan-out it does not bound is
bounded by `rule_read_max` alone.

### 8.3 Mutability classes

| Class | Content | Removal | Monotonic reads |
|---|---|---|---|
| Mutable, the default | Updatable | Delete | None guaranteed |
| `@bitemporal` | Revised: an update is a new version of the same instance, every version kept | A retraction closes `asserted` | `believed at` a past instant is immutable; `with retracted` reads not reading `asserted` only grow |
| `@append_only` | Immutable | Never, except by a legal purge | All reads of the type |

`@immutable` on an attribute makes that value immutable within a mutable instance. On a subtype of a `@bitemporal`
type, `@append_only` also refuses retraction (`UGS1013`), so its instances never leave the live slice and its reads are
monotonic. An `@append_only` relation may not
lose instances through a participant's deletion: each participant type must be `@append_only` or `@bitemporal`, or the
relation must declare `restrict` on that role, since `cascade` deletes the instance and `orphan` writes it (`UGS0601`). A retraction of a participant never writes it: the
instance of a relation that is not `@bitemporal` is live while every participant of it is live (section 8.4, rule 6),
so it leaves the default read with the retracted participant and stays as written.

### 8.4 Bitemporal statements

```
abstract statement Asserted : Statement @bitemporal(valid: valid) {
  valid: Interval<DateTime> Optional @index(interval)
}
```

1. **Transaction time is the engine's.** Each instance of a `@bitemporal` type gains the read-only interval `asserted:
   Interval<DateTime>` and the Boolean `live`. `asserted` opens at the epoch of the commit inserting the instance and
   closes at the epoch of the commit retracting it; it is stored as the two epochs (section 12.4) and read as the
   wall-clock times the sequencer recorded for those epochs' closes, so `believed at $t` resolves `$t` to the last epoch
   closed before `$t` and compares epochs. `live` is `asserted.end is none`.
2. **An update is a revision.** A write that changes an instance's attributes or an optional or set-valued role
   closes the current version's `asserted` at the commit and opens a new version of the same instance, with the same
   identity, key and handle, so `believed at $t` reads the version current at `$t` and the live slice reads the
   newest. The engine already stamps every write with a commit version and resolves an instant to an epoch; a
   revision is one more version of the same instance, kept for the type's whole history rather than the retention
   window, so the statement every edge, pointer and handle names stays the same statement.
   A correction that must not keep the identity is a retraction plus a new instance. A revision is how an update is
   stored and no rule of its own: a key attribute, an `@immutable` attribute and a required role stay as section 8.2
   holds them, each refused under its own number on a revision as on any update (`UGS1007`, `UGS1012`, `UGS1025`),
   and an `@append_only` type admits no revision (`UGS1013`).
3. **Retraction, not deletion.** `retract` closes `asserted` at the commit; the instance stays readable, and is never
   physically removed except by a legal purge (section 8.8). A retraction of an instance of a type that is not
   `@bitemporal` is refused (`UGS1026`).
4. **Reads see live instances by default.** `believed at $t` reads the instances whose `asserted` contains the epoch of
   `$t`, retracted ones included; `with retracted` reads every instance; `open` keeps live instances whose valid
   interval has no end.
5. **Constraints count live instances:** `@set`, cardinality, `@limit`, `@no_overlap` and `@acyclic`. Keys and
   `@unique` count retracted instances too, whose claims are permanent (section 7.2). `@no_overlap` on a bitemporal type
   compares the declared valid time.
6. **`@on_retract(role: restrict | restrict open | cascade)`** replaces `@on_delete`: `restrict` refuses a retraction
   while live instances name the participant in that role, `restrict open` while open ones do and, in the same commit,
   retracts the live ones that are no longer open, and `cascade` retracts them in the same commit. The default is
   `cascade`. An insert naming a retracted participant is refused (`UGS1019`). The cascade reaches `@bitemporal`
   relations alone, the ones with an `asserted` to close. A relation that is not `@bitemporal`, an `@append_only`
   provenance link or a mutable one, is not written by a retraction: its instance is **live while every participant of
   it is live**, derived from the participants' `asserted` intervals and never stored on it, so it leaves the default
   read with the retracted participant and returns under `with retracted` beside it, and `UGS1019` keeps a new one from
   naming the participant afterwards. An `@append_only` relation is therefore never written by a cascade, which is what
   `UGS0601` admits a `@bitemporal` participant for. Every participant type of a `@bitemporal` relation is itself
   `@bitemporal` or an `@append_only` entity type (`UGS0602`): a participant is retracted, never deleted, and none is
   a relation that is not `@bitemporal`, whose instance would leave the live slice with its own participant while the
   relation naming it stays, so no version of the relation names an instance that is gone or out of its slice, and the
   live slice and every `believed at` slice are closed under references through every role but a `@retracts` one
   (rule 7; `SCHEMA-APG.md`, proposition 15.2). A catalog type as a participant is a value and takes no part in the rule.
7. **`@retracts(role)`** on a relation type: inserting an instance retracts its participant in that role in the same
   commit, so a correction is one statement; the participant must be live (`UGS1019`). The role is a reference into
   history, outside `@on_retract`: the instance stays live naming the participant it retracted, and a read following
   the role reaches that participant as `with retracted` reads it, so a read's slice holds it beside the live instances
   (`SCHEMA-APG.md`, proposition 15.2).
8. **`@ends(role)`** on a relation type with a valid interval: inserting an instance closes the valid interval of its
   participant in that role at the instance's own valid start, with that bound's precision, in the same commit, so a
   succession is one statement and the old statement stays believed. The instance must state a valid start, and the
   participant must be live with a valid interval that begins before that start and is open or ends after it
   (`UGS1035`).

**History retention.** No instance operation, a delete or a retraction, physically discards an instance of a
`@bitemporal` or `@append_only` type: a legal purge removes named instances (section 8.8), and a Drop removes a
container, a graph or a tenant's data, with everything in it, history included, under the action `drop`, naming the
catalog version it drops under and recorded as an audit event (`../binary_protocol/PROTOCOL.md` section 6.5). So `believed at`,
`as of epoch` and `as of time` are answered for these types at any past point of a graph that stands. Other types keep
their versions for the configured retention window, and a read beyond it is refused naming the window (`../binary_protocol/PROTOCOL.md`
code 3).

### 8.5 Delete semantics

An entity delete removes the entity and applies each relation's `@on_delete` action to the instances naming it:

| Action | Effect | Allowed when |
|---|---|---|
| `cascade`, the default | The instance is deleted, and so are instances of relations over it, recursively, in one commit | Always |
| `restrict` | The delete is refused while instances name the participant in that role, live or not, since one that is not live still reads under `with retracted` (`UGS1020`) | Always, except on a `_` side |
| `orphan` | The instance stays with the role empty | Only on an `Optional` single-valued role of a relation that is not `@append_only` |

A deleted entity's IRI may be created again, since a delete releases its key's and its values' claims; the new instance
is a new ID, whose record names the ID it replaced, while the tombstone holds the version of the delete: a read naming
the IRI at a version below that delete resolves the instance that held it then, never an absence. A delete writes a tombstone at the entity's
home, so a later write naming the deleted entity as its subject is refused (`../binary_protocol/PROTOCOL.md` code 30,
`EntityDeleted`), and one naming it as a participant with `UGS1019`, the participant check answering it deleted
(section 12.3). A relation instance delete
removes the instance and cascades to relations over it; an edge-encoded instance that holds no reifier is deleted by
the delete of its quad. Deleting a `@bitemporal` instance is a retraction (section 8.4).
No delete reaches a participant of a `@bitemporal` relation, whose participant types are all `@bitemporal` or
`@append_only` entity types (`UGS0602`). The compiler warns when `restrict` forces coordination that `@colocate` on the same role
would avoid (`UGS0903`).

### 8.6 Filtered and per-type bounds

```
@limit(aggregate op n per (roles [, type]) [where condition])
```

`aggregate` is `count()` or `sum(attr)`; `op` is `<=`, `>=` or `=`; `n` a literal. The bound holds, per combination of
the participants in the `per` roles, and per concrete relation subtype when `type` is listed, over the live instances
satisfying `condition`, which reads the instance's own values, its participants' types and `open`. A conjunct of
`condition`, a term joined to the rest by `and` at its top level, that reads only the types of the participants in the
`per` roles **selects the groups**: the bound applies to the combinations whose participants satisfy it, and the other
conjuncts filter the instances counted. `count() >= 1 per delegate where delegate is Software and open` asks every
Software delegate for an open instance and asks nothing of a Person. An upper bound reads the same either way, since a
group the selection leaves out counts no instance. A lower bound, `>=` or `=` with `n ≥ 1`, names one `per` role and
no `type` (`UGS0529`): its groups are the participants of that role's types the selection admits, one no instance
reaches included, whose count is zero. A lower bound is
checked at commit: creating the grouped participant and its first instances happens in one transaction, and a
retraction, delete or valid-window close that would break it is refused (`UGS1023`). Cardinality markers are shorthand
for unfiltered bounds on one role.

**The aggregate is stored.** Each group's count or sum is a record in the marks keyspace of the partition that owns
the group (section 9.3), written in the same commit as the change that moves it, as a count index is: an insert, a
delete or retraction, an update of the aggregated attribute, and an update or valid-window close changing whether the
condition holds for an instance each add or remove that instance's part, and the check reads the one record. A group
no instance reaches holds no record and reads as zero.

### 8.7 Refusals

A refusal travels as `../binary_protocol/PROTOCOL.md`'s Error with code 7 (Refused); section 15 asks the protocol to carry the UGS
refusal number and the instance. The refusals:

| Number | Name | The write |
|---|---|---|
| `UGS1001` | UndeclaredPredicate | names a predicate the catalog does not declare on the subject's type, which is not `@open` |
| `UGS1002` | UnknownType | creates an instance naming no concrete type, or a type the graph's schema does not declare |
| `UGS1003` | AbstractType | names an abstract type as a concrete one |
| `UGS1004` | TypeChanged | names two concrete types in one creation, or writes a type on an existing keyless instance (section 8.1) |
| `UGS1005` | RequiredMissing | leaves a required attribute without a value |
| `UGS1006` | KeyMissing | creates a keyed instance without its key |
| `UGS1007` | KeyChanged | writes another value to a key attribute |
| `UGS1008` | KeyDuplicate | creates an instance, of whatever concrete type of the tree, whose key another instance of the tree holds in the graph: a live one, or any of a `@bitemporal` or `@append_only` type (section 7.2) |
| `UGS1009` | UniqueDuplicate | writes a `@unique` value another instance of the tree holds in the graph: a live one, or any of a `@bitemporal` or `@append_only` type |
| `UGS1010` | ValueCheck | breaks `@pattern`, `@range`, `@values` or `@check`; the message names the annotation |
| `UGS1011` | Datatype | writes a value outside the attribute's type: a datatype mismatch, an integer out of range, a non-finite float, a malformed geometry, an interval whose start is after its end or with both bounds open |
| `UGS1012` | Immutable | replaces an `@immutable` value |
| `UGS1013` | AppendOnly | updates, revises or deletes an `@append_only` instance, or retracts one whose type is below a `@bitemporal` type (section 8.3) |
| `UGS1014` | Cardinality | takes a participant past a navigation name's bound, or below its lower bound |
| `UGS1015` | RoleMissing | creates a relation instance without a participant in a required, single-valued role |
| `UGS1016` | RoleEmpty | creates a relation instance with a set-valued role empty, or empties one by a removal |
| `UGS1017` | ParticipantMissing | names a participant that does not exist |
| `UGS1018` | ParticipantType | names a participant outside the role's types |
| `UGS1019` | ParticipantRetracted | names a retracted or deleted participant, or retracts through `@retracts` a participant that is not live |
| `UGS1020` | Restrict | deletes or retracts a participant while instances restrict it; the message names the relation and role |
| `UGS1021` | Cycle | would close a cycle through an `@acyclic` relation tree |
| `UGS1022` | Depth | would create a path of more relation instances than `max_depth` |
| `UGS1023` | Limit | breaks a `@limit` bound; the message names the bound and the participant |
| `UGS1024` | Overlap | breaks `@no_overlap` |
| `UGS1025` | RoleFixed | replaces the participant of a required, single-valued role, on a relation of any mutability class |
| `UGS1026` | RetractMutable | retracts an instance of a type that is not `@bitemporal` |
| `UGS1027` | Unit | names an undeclared unit, or a unit of another quantity kind |
| `UGS1028` | VectorDimension | writes a vector of another dimension |
| `UGS1029` | DerivedWrite | writes a derived name, or inserts or deletes a triple the RDF view derives from stored ones (section 11.4); the message names the stored triple to write |
| `UGS1030` | EnumMember | writes a member the enum does not declare |
| `UGS1031` | Graph | writes to an undeclared graph, to the default graph of a tenant without one, or creates an entity in a graph that is not of its type's schema |
| `UGS1034` | IncompleteInstance | leaves an instance assembled from triples incomplete: a reifier without its triple or its relation's concrete type, a `@bitemporal` or `@multi` edge-encoded instance's triple without its reifier, a role triple without its instance's type, a triple of a node-encoded relation outside its instance; a missing required role is `UGS1015` |
| `UGS1035` | Succession | inserts an `@ends` instance without a valid start, or whose participant in that role is not live, has no valid interval, or has one that begins after the instance's valid start or ends before it |
| `UGS1036` | AttributeCardinality | writes a second value to a single-valued attribute of one instance |
| `UGS1037` | RetiredIri | mints an IRI equal to the birth IRI an instance holds (section 7.3) |
| `UGS1038` | IriNotMinted | creates an instance under a subject IRI its type does not mint for it: a keyed type's instance under another IRI than its key mints, or a keyless type's instance under an IRI in place of a blank node (section 7.3) |
| `UGS1039` | EvaluationFailed | commits an instance for which an expression of the schema cannot be evaluated: an overflow or a division by zero in a `@check`, a `@limit` condition or a computed derivation (section 4) |
| `UGS1040` | ReadBound | would need a check of `@acyclic` or `@no_overlap` to visit more than `rule_read_max` relation instances (section 8.2); the message names the rule and the bound |

### 8.8 Legal purge

A purge physically removes named instances of `@append_only` or `@bitemporal` types, which no delete or retraction
removes, for a legal reason. It is the `Purge` command (section 15): a list of at most 10,000 instance IRIs, which the
operator computes by a query at an epoch and reviews before sending, under the action `purge` at tenant scope
(`../../openspec/notes/SECURITY.md`). The command is coordinated: it is sequenced at an epoch and executed as an entity delete's shares are
(section 8.5, P24 rule 8), with removal in place of a tombstone's fence. At the instance's home it removes every version
of its attributes, its statement record, its index and derived entries, its dictionary entries and its referrer set; at
every member of its referrer set it removes every edge naming the instance, as a fact about the purged subject, with
their index entries and marks. A relation instance in the list is removed with the relations over it. The metadata
group records the purge as a **purge epoch** with the SHA-256 of each purged IRI, never the IRI, which may itself be
personal data; an auditor verifies that an IRI was purged by hashing it. Reads at epochs before the purge epoch answer
from the remaining data, and a monotonic-fast read never combines data from both sides of a purge boundary. Backups
taken before the purge epoch hold the data and are the operator's to retire. **[stage 8]**; with one partition holding
the tenant's run the command executes locally.

## 9. Placement and write classification

### 9.1 Where an instance lives

| Instance | Bucket, and so partition |
|---|---|
| A keyed entity, a keyed relation instance | Derived from its key (section 7.2); a keyed relation declares no `@colocate` (`UGS0702`), and each of its constraints that reads a participant reads another partition's state |
| A keyless entity | The bucket its handle states: a bucket the creating partition owns, so an entity created with another is created beside it |
| An unkeyed relation instance | Its **anchor participant's** bucket: the participant in the `@colocate(role)` role the type declares or inherits from the nearest supertype declaring one, or in the first declared role (own or inherited, in declaration order) without one |

An instance's attributes, its forward entries, its local reverse entries, its indexes and its statistics are written
in its partition in one local commit (`../binary_protocol/PROTOCOL.md` section 11.11, P24). A navigation from a participant that is not
the anchor is a reverse read: the participant's home reads its referrer set and fans out. The coordination report lists
each such navigation as a scatter read (`UGS0905`); a `@symmetric` relation's navigation name, declared on both roles,
is one scatter read, recorded at the second role. Before an instance's partition first commits a reference to a
participant it does not own, it registers at the participant's home, which adds the partition's bucket group to the
participant's referrer set, unless the role declares `-> _`. A `@key`'s claim lives in the entity's own bucket (section 7.2), so creation and the key check meet in one
partition; a `@unique` index is partitioned by the value's hash (`Bucket::of_value`), so each value has one owning
partition.

### 9.2 The three paths

| Path | Definition | Now | Stage 8 |
|---|---|---|---|
| Coordination-free | Everything the write reads to validate and writes lives in the transaction's partition, and its checks read only the instance itself, its participants' identity and type (answered by the registration pre-step), and the partition's own state | Local commit | Local commit; a DirectWrite of such changes may take the fast path (`UltraGraphDB Distribution Protocol.md`) |
| Segmented | The write's check reads state one partition owns: a key's or `@unique` value's claim at the value's owner; a bound on a participant at its home. Local when that partition is the transaction's | Local when the owner is the transaction's partition; otherwise refused with `CrossPartition` | Sequenced through the sequencer, validated at the owner |
| Coordinated | The write must atomically read or change state in several partitions: a `restrict` or `cascade` delete reaching other partitions, an `@acyclic` edge whose paths cross partitions, `@retracts` across partitions, a lower-bound or `@limit` check whose instances are spread | Local when everything the write reads and writes is in the transaction's partition; otherwise refused with `CrossPartition` | Sequenced, Calvin-style |

### 9.3 Classification per constraint

| Constraint | State read | Owned by | Path |
|---|---|---|---|
| Concrete type, declared predicate, required, datatype, enum, unit, vector dimension, `@pattern`, `@range`, `@values`, `@check`, `@immutable`, `@append_only`, `@bitemporal` content, type fixed | The instance | Its partition | Coordination-free |
| Participant exists, live, of type | The participant's status and prefix | Its home, answered by the registration pre-step | Coordination-free |
| `@key` | The key's claim | The entity's own bucket, hashed from the declaring prefix and the canonical key (section 7.2) | Segmented, local: the creating partition owns it |
| `@unique` | The value's claim | The partition owning `Bucket::of_value(tenant, predicate, value)` | Segmented |
| Cardinality on the anchor role's navigation | The participant's count through the role | The anchor's partition, which holds every instance of that participant | Segmented, local |
| Cardinality on another role's navigation | The participant's instances, anchored elsewhere | Spread | Coordinated |
| `@set` | The combination's existing instance | The anchor's partition | Segmented, local |
| `@acyclic`, `max_depth` | The relation's paths through the inserted instance | The partitions holding the instances along them | Coordinated |
| `@limit` per the anchor role | The aggregate | The anchor's partition | Segmented, local |
| `@limit` per another role | The aggregate over spread instances | Spread | Coordinated |
| `@no_overlap(per: roles)` with the anchor role among them | The combination's intervals | The anchor's partition | Segmented, local |
| `@no_overlap` without the anchor role | Spread | Spread | Coordinated |
| `restrict` or `restrict open` on a role whose relations are all anchored on it | The instances naming the participant, and under `restrict open` the retraction of the live ones no longer open | The participant's home | Segmented, local |
| `cascade` on a role whose relations are all anchored on it | The instances naming the participant | The participant's home | Coordination-free: the deletes are writes in that partition |
| `restrict`, `restrict open`, `cascade` otherwise | Instances in referrers' partitions | The referrer set's members | Coordinated; stage 8 sends sequenced shares |
| `orphan` | The instance | Its partition | Coordination-free |
| `@retracts(role)` | The participant's `asserted` | The participant's home | Segmented, local when colocated with the role; coordinated otherwise |
| `@ends(role)` | The participant's valid interval | The participant's home | As `@retracts` |
| Index and materialized derivation maintenance | The instance's partition | Its partition | Coordination-free |

### 9.4 How the classification is built

1. Enumerate every write operation the catalog version permits: per entity type, create, update of each attribute,
   delete; per relation type, insert, update of each attribute, change of each optional or set-valued role, delete, and
   for `@bitemporal` types retract in place of delete, their updates being revisions, which are listed.
2. For each constraint, list the operations that could violate it: those writing what it reads, an update of an
   attribute its condition reads among them, as a `@limit`'s `where` reads `rank`. Two operations **conflict** on a
   constraint when concurrent executions of them in different partitions could both pass their local checks and
   together violate it.
3. Build the conflict graph: operations as nodes, an edge per conflict, labelled with the constraint and the partition
   that owns its state.
4. Take each relation's anchor as section 9.1 states: the role its `@colocate` names, own or inherited, else its first
   role; the compiler chooses no anchor of its own. A constraint the anchor does not make local is routed through
   coordination. When the anchor keeps another anchor-dependent constraint of the relation local, `UGS0904` names the
   pair, as section 16's `Supply` keeps its `restrict` on `supplier` local and coordinates its `@limit` per `buyer`; a
   lone constraint only another anchor would make local, with none competing, is coordinated with no `UGS0904`, and a
   `restrict` that `@colocate` would make local is reported as `UGS0903`. A subtype's own `@colocate` decides for that
   subtype whatever its supertypes' constraints demand, and the report says what the kind gave up.
5. Assign each operation the most demanding path among its constraints' rows in section 9.3, emit the report, and
   store the table in the catalog version.

### 9.5 The coordination report

Every successful compilation emits a report, `UGS0901`, listing: each write operation with its path and the constraint
deciding it; each scatter read (`UGS0905`); each placement conflict and its resolution (`UGS0904`); each `restrict`, and no other
constraint, whose coordination `@colocate` on its role would avoid (`UGS0903`); each declared construct this deployment does not enforce
(`UGS0902`); and, per write operation, the index entries it writes, value, interval at two per attribute, count,
fulltext and vector, with the ten operations writing most under `UGS0906`. A version installed over a previous one also
lists which operations changed path. The report is returned
with the install's reply and recorded in the catalog version.

The report is data. Its content is the catalog version's Classification and Report entries (section 10.2), one record
per line of the table below, and those records are what a test, a client and Describe read. The text an Install's
reply carries is their rendering and nothing more: one line per record, in the entries' canonical order, each the
note's code, the record's subject as a qualified name, then its fields as `name=value` pairs separated by one space,
in the order the table lists them, a value in its canonical lexical form. Equal records render to equal text.

| Record | Code | Subject | Fields |
|---|---|---|---|
| A write operation | `UGS0901` | The type and the operation: `Engine.create`, `Engine.update.status`, `Owns.insert`, a role change as `{type}.role.{role}`, `Engine.delete`, `MemberOf.retract` | `path`: `coordination-free`, `segmented` or `coordinated`; `local`: whether the state its deciding constraint reads is the transaction's partition's by placement; `by`: the deciding constraint, or `none`; `indexes`: the index entries it writes |
| An unenforced construct | `UGS0902` | The item declaring it | `construct`; `stage`, the stage that enforces or executes it |
| A `restrict` colocation would make local | `UGS0903` | The relation and the role | `colocate`, the role to name |
| A placement conflict | `UGS0904` | The relation | `local`, the constraint kept local; `coordinated`, the constraint routed through coordination |
| A scatter read | `UGS0905` | The type and the navigation name | `relation`, its qualified name; `role` |
| A heaviest writer | `UGS0906` | The type and the operation | `indexes`; `rank`, 1 to 10 |

### 9.6 What the schema gives request typing and query classification

The compiled catalog carries, per predicate, the facts the frontend decides a request's path from before it reads a term
(`../binary_protocol/PROTOCOL.md` section 11.2a; `UltraGraphDB Distribution Protocol.md`, "What qualifies"): whether a write of it is
blind (coordination-free, no check that reads beyond the instance), whether it needs a unique claim, whether it is
single-valued, whether a check or a bound reads it, whether it is derived, and its mutability class. Per type it carries
the mutability class, `@access`, and whether creating an instance of it is blind. The query classifier combines these with the query: a query reading only
`@append_only` and `@immutable` data may run monotonic-fast with every result true and lasting; one reading mutable data
monotonic-fast is flagged; one filtering through an `@access` relation, an aggregate or a negation runs at a snapshot.

**One-frame writes.** In a governed tenant a DirectWrite carries changes whose every predicate is blind, and a
ConditionalWrite such changes with its commit-time operations (`../binary_protocol/PROTOCOL.md` sections 6.6 and 6.7).
A rule over the instance's own fields, a datatype, a value check or a `@check`, is validated in the frame's one turn
and refuses the frame with its number. What takes a read-write transaction is a claim of a key or a `@unique` value
and a rule that reads beyond the instance, a bound, a `@limit`, `@no_overlap`, `@acyclic` or a participant's field; a
frame holding such a change is refused naming one.
Insert-if-absent and upsert-and-link, which create a node of no declared type, are refused in a governed tenant.

## 10. Compilation and the catalog

### 10.1 Installing a schema

An install names a tenant, the catalog version it builds on, and the sources it adds or replaces: whole schemas and
modules, each source replacing the one of the same name. The compiler runs the phases below on the full set of sources
the new version holds; a failure in any phase refuses the install whole with its diagnostics and changes nothing.
An Install into a tenant that holds a schema is served from stage 9.5 **[stage 9.5]**: until then a tenant takes one
Install, of one or more sources compiled together, and a later one is refused naming the stage (section 14).
Installation takes the `declare` privilege on the tenant (`../../openspec/notes/SECURITY.md`). A tenant's first schema is installed
into a tenant that holds no data written before it: an Install into a tenant that holds a statement, a write-path
declaration or a default-entry record is refused, the message naming the Drop that empties the tenant
(`../binary_protocol/PROTOCOL.md` sections 6.5 and 8.4), since such data is `Resource` data no declared type claims. From
the Install's check of the tenant until its version takes effect, every write to the tenant is refused as fenced and
runs again once the schema governs it (`PROTOCOL.md` code 31), so no data is written between the two.

| Phase | Does | Refuses with |
|---|---|---|
| 1 Lexing and parsing | Sections 3 and 4 | `UGS0001`–`UGS0099` |
| 2 Resolution | Names, imports, modules, inheritance | `UGS0100`–`UGS0199` |
| 3 Typing | Types, fields, defaults, annotations' arguments, expressions | `UGS0200`–`UGS0299` |
| 4 Identity | Keys, templates, bases | `UGS0300`–`UGS0399` |
| 5 Relations and constraints | Roles, navigation names, satisfiability, mutability consistency | `UGS0400`–`UGS0699` |
| 6 Placement and classification | Sections 9.1–9.4 | `UGS0700`–`UGS0799` |
| 7 Views | Vocabulary IRIs, GQL names | `UGS0800`–`UGS0899` |
| 8 Evolution | The difference from the previous version, section 13, and a migration source installed with the version, as `SCHEMA-EVOLUTION.md` section 5 checks it | `UGS2001`–`UGS2099`, `UGS2101`–`UGS2199` |
| 9 Allocation | Prefixes for new types and sequences for new predicates, from the metadata group's next free ones | `UGS2005` |
| 10 Emission | The catalog version, the views, the report; the version decoded by `CATALOG.md` section 6 and encoded back to the same bytes | |

The compiler runs beside the metadata group, never on the thread that sequences its log: it takes the tenant's base
version, next free prefix and next free predicate sequence as its inputs, so the version it emits is decided before
the group sees it, and the group commits that version only while all three are still the tenant's. A version failing
phase 10's decode or re-encoding is a fault of the compiler, never of the sources: the install is refused with
`../binary_protocol/PROTOCOL.md` code 7, the message naming the decoder's step and offset, and nothing is allocated or
committed.

The version is committed as pending through the metadata group and takes effect at the epoch `../binary_protocol/PROTOCOL.md` section
11.13 states; a tightening change is checked by the validation scan at that epoch (section 13.3), and the reply waits
for it. The reply carries the version number, its effective epoch and the report, or the diagnostics.

### 10.2 The compiled catalog

A catalog version holds the entries below, in one canonical encoding: entries ordered by kind, then by name as UTF-8
bytes; every field present; a decoder refuses any other byte sequence, so a version has one encoding and the schema id
(section 10.3) is stable. The metadata group treats the bytes as opaque.

**The bytes.** The layout is specified before it is implemented, in `CATALOG.md` beside this file, and holds to six
rules:

1. **The frame's discipline.** A version is a body in `../binary_protocol/PROTOCOL.md` section 4.1's layout: a fixed part, a
   section table, 8-aligned sections, zero padding and u32 indices into a string list. Its entry table is Describe's
   entry record, kind, name, offset and length (section 8.3 there), so Describe carries a version's entries without
   encoding them again.
2. **Each entry self-contained.** An entry's bytes are again such a body, with a fixed part per kind and its own
   strings, since Describe carries any subset of the entries to a principal and the schema id hashes them as stored.
3. **Verified when read.** The fixed part holds a format version and an XXH3-64 of the bytes after it. A decoder checks
   the checksum, then every rule of the one encoding, the order, the offsets, the zero bytes and the strings'
   interning order, and refuses anything else. A role that cannot decode a version its log or its metadata group names
   stops, so an install decodes the version it compiled before committing it (section 10.1).
4. **Laid out for the commit.** Beside the entries in canonical order, a version holds an index from type prefix to
   Type entry and one from a declared predicate's sequence to its Predicate entry, so a partition reaches a type's
   fields, roles, checks and prefix ranges by index and a predicate's declaration from its ID (section 7.1). What a
   role holds decoded is views over the bytes, built once and shared by its threads, and nothing is parsed or looked
   up by name at a commit.
5. **Expressions as verified programs.** An instance check, a `@limit` condition and a computed derivation, the
   expressions a commit evaluates, are stored as typed postfix programs of fixed-width instructions over the entry's
   fields and constants. The decoder verifies each program's stack discipline, its operands' bounds and its depth, at
   most the grammar's nesting bound of 64, and evaluation is a loop with no recursion and no allocation. A role
   evaluating one needs no parser. A function's body and a path derivation's are queries, which the query core plans
   from stage 7: each is stored as its text, type-checked, until that stage gives it a compiled form.
6. **Expectations independent of the code.** The layout's file works one example by hand, a schema of one entity with
   one attribute, byte by byte. Larger versions are checked entry by entry and field by field against expectations
   written from this file, through the decoder, and their bytes pinned once those checks pass.

| Entry | Holds |
|---|---|
| Schema | Name, version, vocabulary base, `@open`, imports with versions, reserved names, doc |
| Module | As a schema, with what it extends and the minimum version |
| Type | Name, kind (statement, entity, relation), prefix, parent, abstract, open, key attributes, IRI template, graph, mutability class, encoding (edge or node, relations), set or multi, symmetric, anchor role, acyclic with its options, valid attribute, bitemporal, retracts role, ends role, access, per-role delete and retract actions, limits, no-overlap declarations, instance checks, `@maps_to`, doc |
| Prefix ranges | Per type, its prefix set as sorted ranges |
| Attribute | Declaring type, name, predicate IRI, type, optional, default, immutable, checks, indexes, `@maps_to`, doc |
| Predicate | IRI, kind (property or edge), cardinality (single or multi), datatype, unique, tokenizers, count index, vector index with its dimension and options, domains: what `../binary_protocol/PROTOCOL.md` section 4.6's kind-8 record carries; and the request-typing facts (section 9.6). One per attribute name per base, per role of a node-encoded relation, per anchor navigation name of an edge-encoded relation and per interval bound |
| Role | Relation, name, participant types, optional, set-valued, super role, navigation name and cardinality or none, predicate IRI, actions, doc |
| Navigation | Type, name, relation, role, direction (anchor-forward or reverse), cardinality, what it yields |
| Enum, record, named attribute type, quantity, unit | Their declarations, with a unit's conversion |
| Derivation | Type, name, body, result type and cardinality, materialized, index |
| Function | Type or free, name, parameters, result, body, monotonic |
| Graph | Name, schema, IRI, base, default |
| Classification | Per type, each write operation's path, its deciding constraint and the index entries it writes |
| Report | The notes of section 9.5 |
| Sources | The installed sources' text, by name, so Describe can return them |
| Retired name | A name a type, attribute, role, navigation name or graph held in an earlier version, with the item holding it now or the version that dropped it; a key space's name; and the deprecation window a migration gave it (`SCHEMA-EVOLUTION.md` sections 4.2 and 9) |
| Migration | Each migration a version was installed by: its source, the source and target versions, the cutover epoch and the run's state, read for the target's history before the cutover (`SCHEMA-EVOLUTION.md` section 7) |

**Attribute identity.** An attribute name is one predicate per vocabulary base: `Organization.name` and `Person.name`
share the predicate `{base}name`. Every type declaring the name, a subtype redeclaring an inherited one among them
(section 6.3), must declare it with the same type and the same
`@index`, `@immutable` and `@maps_to` (`UGS0215`), unless `@iri` on one gives it a predicate of its own, after which
the agreement rule applies per predicate; requiredness, defaults and value checks are per declaring
type. A predicate carrying `@key` or `@unique` is declared by one type tree only (`UGS0216`), so the predicate's
`unique` flag is exactly the tree's rule; a declaration that `@iri` gives a predicate of its own is outside another
tree's claim, as Gen5's `Teloid.action_identifier` is outside `Action`'s. A subtype's inherited attribute is the same predicate.

The catalog's existing kinds 8, 9 and 10 (`../binary_protocol/PROTOCOL.md` section 4.6) are derived from the Predicate, Type and Role
entries: an entity type's kind-9 record carries its parent and abstract flag; a relation's kind-10 record carries its
role predicates. Section 15 asks the protocol for the record kinds that carry the rest.

### 10.3 Schema id, Describe and introspection

The schema id is `../binary_protocol/PROTOCOL.md` section 11.7's hash of the Describe body, which section 15 asks to carry every entry
the principal may read: Type, Prefix-range, Enum, Record, Graph and Schema entries whole, and Attribute, Predicate,
Role, Navigation, Derivation and Function entries only where the principal holds `read` on every predicate they name
(`SECURITY.md`, Describe). Introspection exposes the catalog as the read-only `schema.` graph: `schema.types`,
`schema.attributes`, `schema.relations`, `schema.roles`, `schema.navigations`, `schema.enums`, `schema.functions`,
`schema.derivations`, `schema.graphs`, `schema.quantities`, `schema.units`, `schema.report`, each with `doc`;
introspection names no prefix, bucket or ID.

### 10.4 Diagnostics

A diagnostic is an error, a warning or a note, with a code, a source location (file, line, column, length), a reason,
and where one exists a concrete fix, in the style of Rust's compiler:

```
error[UGS0601]: append-only relation `Supply` would lose instances on delete
  --> aviation.ugs:51:13
   |
51 |   item:     Asset                   -> contracts*
   |             ^^^^^ `Asset` is deletable and `item` defaults to `cascade`
   |
help: keep `Supply` append-only by restricting deletes of its participants
   |
54 |   @on_delete(supplier: restrict, buyer: restrict, item: restrict)
```

A location names the source's file, the line and column of its first character, and its length. Lines and columns
count from 1: a column is one more than the Unicode scalar values before the location on its line, and a length is
the scalar values the location spans; a line ends at its `\n`, and the `\r` of a `\r\n` belongs to no column. A
diagnostic's location is the smallest span naming what breaks the rule: the token, name, annotation or argument at
fault, and for a rule over a declaration as a whole, the declaration's name. A cycle, of inheritance (`UGS0105`) or
of calls (`UGS0118`), is one diagnostic, at the name of the first of its declarations in source order. A source that
is not UTF-8 is located at its first invalid byte, one column of length 1; an unterminated string, comment or IRI at
its opening delimiter; an over-long line at the whole line. A fix is one or more edits, each a location and the text
replacing it, an insertion having length 0, so a tool applies it as given.

A schema with any error is refused. Warnings and notes accompany a successful install in the report. One condition
takes one code: the most specific code naming a condition is raised, and a general one, `UGS0101` first, only where no
other names it. The codes:

| Code | Kind | Condition |
|---|---|---|
| `UGS0001` | error | A character outside the lexical structure, or a byte sequence that is not UTF-8 |
| `UGS0002` | error | An unterminated string, comment or IRI |
| `UGS0003` | error | An invalid string escape |
| `UGS0004` | error | A malformed number |
| `UGS0005` | error | A temporal literal naming no instant, or a duration with no part |
| `UGS0006` | error | Outside an expression position, a token the grammar does not expect; the message names what was expected, and an identifier differing from an expected keyword by case alone names that keyword as its fix |
| `UGS0007` | error | A reserved word used as an identifier |
| `UGS0008` | error | An unknown annotation |
| `UGS0009` | error | An annotation argument of the wrong form, count or type; a `prefix` base that is not an absolute IRI ending in `#` or `/` |
| `UGS0010` | error | An annotation repeated on one item |
| `UGS0011` | error | An annotation on an item its section does not allow |
| `UGS0013` | error | A source past its size, line length or nesting bound |
| `UGS0101` | error | An unresolved name that no code below names more specifically |
| `UGS0102` | error | A duplicate declaration, graph `@default`, `prefix`, or a clash with a built-in |
| `UGS0103` | error | `import schema` names a schema the tenant does not hold |
| `UGS0104` | error | `@maps_to` names a prefix no `import` declares |
| `UGS0105` | error | An inheritance cycle |
| `UGS0106` | error | A parent of the wrong kind: an entity extending a relation, a relation an entity, a statement anything but a statement; a module declaring a root type |
| `UGS0109` | error | A module or `import schema` version bound the tenant does not meet |
| `UGS0110` | error | A reserved name, or the prefix `schema`, declared or imported |
| `UGS0111` | error | A name two modules declare |
| `UGS0112` | error | An undeclared enum member in a default or literal |
| `UGS0113` | error | An unknown or ill-formed quantity or unit: `canonical` not a unit of the kind, `quantities` naming an unknown kind |
| `UGS0114` | error | An annotation naming a role the relation lacks; `@colocate`'s is `UGS0701` |
| `UGS0115` | error | An annotation naming an attribute the type lacks |
| `UGS0116` | error | A graph naming an unknown schema |
| `UGS0117` | error | A call of an unknown function |
| `UGS0118` | error | A cycle in the function call graph |
| `UGS0119` | error | A function body naming an unknown parameter or field |
| `UGS0120` | error | In an expression position, a token the expression grammar does not expect; or an operation version 1 leaves out, measure arithmetic beyond section 5.8's among them |
| `UGS0201` | error | An unknown type name, a parent's or a participant's among them |
| `UGS0202` | error | Type arguments of the wrong count or kind |
| `UGS0203` | error | `Decimal(p, s)` outside `1 ≤ p ≤ 38`, `0 ≤ s ≤ p`, `s ≤ 18`, `p − s ≤ 20` |
| `UGS0204` | error | A vector dimension outside 1 to 65,535 |
| `UGS0205` | error | A collection or record element the type does not allow |
| `UGS0206` | error | An `Interval` over a type that is not `Date`, `DateTime`, `LocalDateTime`, an integer type or a `Decimal` |
| `UGS0207` | error | A literal, default, derivation or body whose type does not match |
| `UGS0209` | error | A default on an `Optional` field |
| `UGS0211` | error | `@pattern` on a type that is not text or `Iri` |
| `UGS0212` | error | `@range` on an unordered type, or with `min > max` |
| `UGS0213` | error | A subtype redeclaring an inherited attribute with another type |
| `UGS0214` | error | A subtype or module loosening an inherited constraint |
| `UGS0215` | error | One attribute name declared by two types with different types or index, immutability or mapping annotations, without `@iri` splitting them |
| `UGS0216` | error | A `@key` or `@unique` attribute's predicate declared by a second type tree |
| `UGS0218` | error | A `Measure` naming a unit of another quantity kind |
| `UGS0221` | error | A duplicate enum member |
| `UGS0222` | error | A duplicate record field |
| `UGS0223` | error | `LocalDateTime` compared or combined with `DateTime` |
| `UGS0224` | error | `Date` compared or combined with `DateTime` without a conversion |
| `UGS0225` | error | `today()`, `now()` or another non-deterministic function in `@check`, `@limit`, a derivation or a function |
| `UGS0226` | error | `@check` reading another instance, or a mutable field of a participant |
| `UGS0227` | error | A computed derivation reading a navigation, a role, another instance or a function that does |
| `UGS0228` | error | An expression that no instance can evaluate: a division by a literal zero |
| `UGS0301` | error | A key attribute that is `Optional` |
| `UGS0302` | error | A key on a subtype of a keyed type |
| `UGS0304` | error | An `@iri` template from which the key cannot be read back |
| `UGS0305` | error | Two `@iri` templates one of which is a prefix of the other |
| `UGS0306` | error | A key attribute of a type that is not keyable |
| `UGS0307` | error | `@key` naming an attribute the type lacks |
| `UGS0308` | error | `@unique` on a collection, record, vector, geometry or measure with a unit on the value |
| `UGS0309` | error | Two schemas or two graphs with one base |
| `UGS0401` | error | A relation with fewer than two roles |
| `UGS0402` | error | A participant naming a declared type that is not an entity, relation or statement type; a union with a repeated member, or with a nested one, a subtype of another member; an undeclared name is `UGS0201` |
| `UGS0403` | error | A role without a cardinality marker, which the parser reports under this code rather than `UGS0006` |
| `UGS0404` | error | A navigation name, derived name or method function clashing with another field of a participant type, inherited ones and the built-in fields of section 5.11 included |
| `UGS0405` | error | `as` naming a role the supertype lacks |
| `UGS0406` | error | A super role specialized twice in one subtype |
| `UGS0407` | error | A specialized participant that is not the super role's type or a subtype, or a specialized role changing the super role's optionality or set-valuedness |
| `UGS0408` | error | `@symmetric` on a relation that is not binary with one participant type and one navigation name |
| `UGS0409` | error | `@set` or `@multi` changed below the root of a relation tree |
| `UGS0410` | error | A union member that is not the super role's type or a subtype |
| `UGS0411` | error | A cardinality or `@limit` on a `_` side; `restrict` there is `UGS0516` |
| `UGS0412` | error | `schema.EntityType<T>` or `schema.RelationType<T>` with `T` of the wrong kind |
| `UGS0413` | error | A navigation name on a role naming a catalog type |
| `UGS0417` | error | A role named as another role or attribute of the relation, or as a built-in field of section 5.11 |
| `UGS0418` | error | An edge-encoded relation whose anchor role declares `-> _` |
| `UGS0501` | error | `orphan` on a role that is not `Optional` and single-valued: a required or a set-valued role |
| `UGS0502` | error | A cardinality `{m, n}` with `m > n` or `n = 0` |
| `UGS0503` | error | `@acyclic` on a relation that is not binary, or whose roles' types share no supertype |
| `UGS0504` | error | `max_depth` outside 1 to 4,096 |
| `UGS0505` | error | `@limit` with an aggregate version 1 does not support, or `sum` over an attribute that is not numeric or that no `@range` or `@check` bounds at or above zero |
| `UGS0506` | error | `@limit` with a bound that is not a literal of the aggregate's type |
| `UGS0507` | error | A `@limit` condition reading other instances |
| `UGS0508` | error | `@no_overlap` or `@acyclic(open)` on a type without a valid interval |
| `UGS0509` | error | `@valid` or `@bitemporal` naming an attribute that is not an `Interval` of the allowed types |
| `UGS0510` | error | `@valid` and `@bitemporal` on one type |
| `UGS0512` | error | `@retracts` naming a role whose participants are not all `@bitemporal`; `@ends` naming one whose participants do not all have a valid interval, or on a relation without one |
| `UGS0513` | error | `@on_retract` on a type that is not `@bitemporal` |
| `UGS0514` | error | `@on_delete` on a `@bitemporal` type |
| `UGS0515` | error | `@on_delete` or `@on_retract` naming a role twice or an unknown action |
| `UGS0516` | error | `restrict` on a `_` side |
| `UGS0518` | error | A subtype removing `@append_only` or `@immutable` |
| `UGS0519` | error | A subtype loosening `restrict` to `cascade` or `orphan` |
| `UGS0522` | error | `@index(interval)` on a field that is not an `Interval` |
| `UGS0523` | error | `@index(geo)` on a field that is not `Geo` |
| `UGS0524` | error | `@index(ann)` on a field that is not a `Vector`, with an unknown metric or quantization, or with a search-time parameter |
| `UGS0525` | error | `@index(fulltext)` on a field that is not `Text` |
| `UGS0526` | error | `@index(range)` on an unordered type |
| `UGS0528` | error | A `@values` literal of the wrong type |
| `UGS0529` | error | A `@limit` lower bound, `>=` or `=` with a bound of at least one, whose `per` names more than one role or `type` |
| `UGS0601` | error | An `@append_only` relation whose participants may be deleted with `cascade`, or that declares `orphan` on a role |
| `UGS0602` | error | A `@bitemporal` relation with a participant type that is neither `@bitemporal` nor an `@append_only` entity type |
| `UGS0701` | error | `@colocate` naming a role the relation neither declares nor inherits |
| `UGS0702` | error | `@colocate` on a relation with `@key` |
| `UGS0704` | error | `@materialized` on a derivation that leaves the partition |
| `UGS0710` | — | Retired 2026-11-10, never raised: its condition is `UGS0116`'s |
| `UGS0801` | error | Two terms minting one IRI |
| `UGS0802` | error | Two names the GQL view cannot tell apart |
| `UGS0803` | error | A malformed `@maps_to` target: a local part outside `PN_LOCAL`, or a relative IRI; a prefix no `import` declares is `UGS0104` |
| `UGS0804` | warning | A documentation comment attached to nothing |
| `UGS0805` | warning | An identifier against the naming convention |
| `UGS0806` | warning | A reference to a `@deprecated` item from another source than the one declaring it |
| `UGS0807` | warning | A `@check` on a type comparing an optional field with no test of its presence, which refuses every instance without the value |
| `UGS0901` | note | The coordination report |
| `UGS0902` | warning | A declared construct this deployment does not enforce or execute yet, with the stage that will: the construct is compiled, checked by every static rule and recorded in the catalog version, and is enforced, built or run from that stage with no schema change |
| `UGS0903` | warning | `restrict` forcing coordination that `@colocate` on the role would avoid, and no other constraint; the fix inserts ` @colocate(role)` after the relation's name and parent |
| `UGS0904` | warning | Two constraints demanding different anchors; which stayed local. One record per relation, located at the coordinated constraint where the relation declares it, else at the relation's name; no fix |
| `UGS0905` | note | A navigation read by scatter; a `@symmetric` relation's navigation name, declared on both roles, is one, at the second role |
| `UGS0906` | note | The write operations writing the most index entries |

## 11. Standard views: RDF, SHACL and GQL

The views present one algebraic property graph (`SCHEMA-APG.md`, section 10). The RDF view is RDF 1.2 with reifiers,
published with SHACL shapes that are cross-checked against the common core of SHACL, ShEx and PG-Schema (Ahmetaj et al.,
2025). The GQL view is a graph of a generated, closed graph type, conforming under ISO GQL's closed graph types and
PG-Schema's STRICT, closed semantics, as MGQL checks conformance for the GQL core. Part 3 of `SCHEMA-APG.md` defines
each view as a function of the algebraic property graph (sections 17 and 18 there), proves the RDF view's round trip
and the GQL view's conformance, and states the test oracle both are checked by (section 19 there); this section states
the views it defines.

### 11.1 Relation encodings

The compiler fixes each relation type's encoding, recorded in its Type entry:

| Encoding | When | Instance form |
|---|---|---|
| Edge | The relation has exactly two roles, both required and single-valued, and carries no `@key`, whose instances take a bucket that is no participant's | One triple `anchor {base}{nav} other`, `nav` the navigation name declared on the anchor role, with an RDF 1.2 reifier, a statement ID carrying the relation's prefix and named by the handle IRI of its type (section 7.3), allocated at insert when the instance carries attributes, the relation is `@multi` or `@bitemporal`, whose `asserted` its statement record holds (section 12.4), its tree holds more than one concrete type, whose reifier's `rdf:type` tells them apart, or a role of any relation in the catalog names the type, a supertype of it or a statement type covering it: the instance then fills that role through its reifier, which the role triple holds as its object term; `@symmetric` relations store the triple once, from the lower ID |
| Node | Every other relation: three or more roles, an `Optional` or set-valued role, or `@key` | A node whose ID has the relation's prefix, named by its IRI (section 7.3), with one role triple `instance {base}{Relation}_{role} participant` per participant and its attributes as its own properties |

An edge-encoded relation's anchor role declares a navigation name, its triple's predicate (`UGS0418`); `@colocate`
moves the anchor to a role that declares one.

The encoding depends on the roles' count, shape and optionality and on `@key` alone: a relation declared
`@multi`, with attributes, with subtypes or named by another relation's role stays edge-encoded, so adding an attribute,
a subtype or a role that names the type never re-encodes. Adding a third role, a set-valued role or `@key` to an
edge-encoded relation, or making one of its roles `Optional`, re-encodes it, which is a destructive change (section 13).
A `@symmetric` relation is stored in ID order under either encoding (section 6.4).

### 11.2 The RDF view

| UGS construct | RDF |
|---|---|
| Entity type | `{base}{Type} a rdfs:Class`, `rdfs:subClassOf` its parent; each instance has `rdf:type` of its concrete type and, answered from the prefix ranges, every supertype |
| Statement type | An `rdfs:Class` whose instances are entities and reifiers |
| Attribute | A datatype property, `rdfs:domain` each declaring type; the literal under the datatype of section 5.1 |
| Enum or `@values` | A literal of the field's type; `sh:in` lists the members |
| `Set<T>` | One triple per member |
| `List<T>`, `record` | One `rdf:JSON` literal |
| `Interval<T>` | `{attr}_start`, `{attr}_end` and the precisions as section 5.1 |
| Edge-encoded `@set` relation without attributes | An `owl:ObjectProperty` `{base}{nav}` from the anchor, with `owl:inverseOf {base}{other nav}` where the other role has one; the inverse is answered from the reverse index |
| Edge-encoded relation with attributes | The asserted triple and an RDF 1.2 reifier of it carrying the attributes, linked as `reifier rdf:reifies <<( s p o )>>`, the triple term RDF 1.2 writes, which is how a query reaches the reifier (`../query/QUERY-SPARQL-CORE.md` section 6.4) |
| Edge-encoded `@multi` relation | One asserted triple; one reifier per instance |
| `@symmetric` relation | An `owl:SymmetricProperty` |
| Node-encoded relation | A class `{base}{Relation}` with one property `{base}{Relation}_{role}` per role; the instance a node of that class |
| Navigation name of a node-encoded relation | A derived object property `{base}{nav}`, `owl:propertyChainAxiom` over the inverse of the gaining role's property and the reached role's property, one triple per instance, answered by the query core and never stored or written |
| Relation subtype | `rdfs:subClassOf` between relation classes; an instance has `rdf:type` of its type and supertypes |
| Specialized role | `{base}{Rel}_{role} rdfs:subPropertyOf {base}{Super}_{superrole}`; the super role's triple is answered from the specialized one |
| Optional or set-valued role | The role triple absent, or several |
| Relation over a relation | The role triple's object is the reifier or relation node |
| Cardinality | `sh:minCount`, `sh:maxCount` on the navigation's shape |
| `@key`, `@unique` | A `sh:sparql` constraint; enforced by the engine |
| `derive` | A read-only property, materialized or virtual |
| `fn` | Not in the view |
| `graph` | A named graph |
| Relation across graphs | A triple in the relation's graph whose object is an IRI of the participant's graph |
| `@bitemporal` | `ugdb:asserted_start`, `ugdb:asserted_end` literals of `xsd:dateTimeStamp` on the instance or its reifier, and `ugdb:live`, an `xsd:boolean` that is `asserted_end`'s absence (section 8.4), `true` on every instance a default read binds |
| Catalog type as participant | The class IRI as an individual, as OWL 2 punning allows |
| `Geo` | A `geo:geoJSONLiteral` |
| `Measure` | The literal of section 5.8; a fixed-unit attribute's property carries `qudt:unit` |
| `@maps_to(ext:term)` | `owl:equivalentClass` or `owl:equivalentProperty` to the external term, in the vocabulary document; on a quantity kind or a unit, an individual, `skos:exactMatch` |
| `///` documentation | `rdfs:comment` |

An `rdf:JSON` literal holds its value as JSON in the canonical form of RFC 8785, the form RDF 1.2 names: a record as an
object of its present fields, an absent optional field left out, an interval as the record of its bounds and
precisions and a measure as the record of its value and unit (`SCHEMA-APG.md` section 13.2); a `List` as an array; a `Set`, where a result carries one as a value, as an
array of its members in the ascending order of their term encodings; an
enum member as a string of its name; a `Bool` as `true` or `false`; and every other scalar as a string of its canonical
lexical form (`../binary_protocol/PROTOCOL.md` section 10.5), so no integer, decimal or float loses
precision.

The view holds a materialized derivation's triples, under the derived name's predicate (section 12.4). `R` of
`SCHEMA-APG.md` definition 17.1 leaves derived names out, so the oracle's comparison of the view with `R(G)` and the
instance corpus's comparison of it leave out the derived names' triples, as the corpus leaves out the built-in
`asserted` and `live`.

The view is published as a vocabulary document per schema and as SHACL shapes: one `sh:NodeShape` per concrete type
with `sh:targetClass`, a `sh:property` per attribute with `sh:datatype` or `sh:class`, `sh:minCount 1` when required,
`sh:maxCount 1` when single-valued, `sh:in`, `sh:pattern`, `sh:minInclusive` and `sh:maxInclusive` from the value
checks, `sh:class` and `sh:or` for roles and unions, and `sh:sparql` for keys and uniqueness.

### 11.3 The GQL view

| UGS construct | GQL |
|---|---|
| Entity type | A node type whose nodes carry the labels of the type and all supertypes; properties as section 5.1, and the read-only property `type`, the concrete type's name as a `STRING`, answered from the prefix on every node and edge |
| Statement type | A label on nodes and relation nodes alike |
| Edge-encoded relation | An edge type from the anchor's label to the other's, directed; properties for the attributes; a `@symmetric` relation an undirected edge type. A type whose instances carry a reifier (section 11.1) is presented twice, as one element with one identity: the edge, and a **relation node** with the type's labels, the same properties, and two derived, read-only role edges `{Relation}_{role}` from it to the participants, the shape a node-encoded relation has, so that a relation can name the instance, since a GQL edge cannot be an endpoint. The edge and the node answer one `ELEMENT_ID`, the reifier IRI, and a write through either is a write to the instance. A type without a reifier has no node presentation: the view follows the compiler's reifier decision and never makes one |
| Node-encoded relation | A node type for the relation and one edge type per role, labelled `{Relation}_{role}`, the relation the type declaring the role, and for a specialized role the super role's `{Relation}_{role}` too; `rdf:type` and the labels are answered from the reifier's or node's prefix alike |
| Navigation name of a node-encoded relation | A derived, directed, read-only edge type from the participant that gains the name to the participant the name reaches, with no properties and no element id, answered by the query core from the two role edges |
| Union role | A label expression `A\|B\|C` |
| Enum | `STRING` with a value check |
| `@bitemporal` | `asserted` and the valid interval as `RECORD` properties |
| Catalog type as value | A type-name `STRING` |
| `graph` | A GQL graph of the generated graph type, exposed together with the graphs its relations reach as one graph |
| Cardinality, keys, uniqueness | Enforced by the engine, with PG-Keys' semantics (section 8.2); ISO GQL's graph types declare none of them |
| `derive` | A read-only edge type or property |

The generated graph type is closed. It holds one node type per concrete entity type and per relation-node type, with
the exact label set of the type, its supertypes and the statement types covering it, and its declared properties; and
one edge type per edge-encoded relation, role edge and navigation edge, whose endpoints list the node types of every
concrete type its role admits. A supertype is a label, never a node type of its own: under closed types a node
conforms only when its label set equals the type's (PG-Schema, section 4.2), so a closed supertype would admit no
subtype's node. Every element of the view conforms to the node or edge type of its concrete type. The graph type names
every label and property; names that would collide are refused at compile time (`UGS0802`).

### 11.4 Writes through SPARQL and GQL

Triples and edges written in one transaction assemble into instances (section 8.1), and every instance is validated
whole at commit. A SPARQL Update may write one role triple at a time within the transaction; the instance is checked
once, at commit. That holds of an instance a request names by its IRI; one created under a blank node is created whole in one
request (section 8.1). A write names the stored triple: an edge-encoded relation's triple under its anchor's navigation name, and a
node-encoded relation's own role property. An inverse navigation property, a node-encoded relation's navigation
property and a super role's property are derived (section 11.2), and a triple of one, inserted or deleted, is refused
(`UGS1029`), so one fact has one written form, owned by its subject's partition. GQL `INSERT` of a node with its label creates the entity of that concrete type; `INSERT` of an edge of
an edge-encoded relation creates the instance; a node-encoded relation is inserted as its node with its role edges.
`SET` on an `@immutable` or key property, `REMOVE` of a required property, and `DELETE` of a restricted participant are
refused as sections 8.2 and 8.5 state; `SET` or `REMOVE` on an instance of a `@bitemporal` type is a revision
(section 8.4), and `DELETE` of one is a retraction. `DETACH DELETE` is an entity delete with `cascade` on every role that declares
it; it is refused where a role declares `restrict` (`UGS1020`).

## 12. Storage mapping

### 12.1 Entities

An entity is its system ID; the dictionary maps its IRI to the ID at its home. Its concrete type is its prefix: no
`rdf:type` quad is stored for a declared type, and `rdf:type` reads are answered from the prefix ranges. Each attribute
value is a quad `(graph, entity, predicate, literal)` in the forward keyspace, single-valued attributes with store
cardinality `Single`, `Set` attributes `Multi`; every literal takes the reverse entry the quad layout gives it. A
single-attribute `@key`'s predicate carries the `unique` flag, a composite key's claim being on the key as a whole, and
a key's claim lives in the entity's own bucket (section 7.2); a
`@unique` attribute's claim lives at `Bucket::of_value`.

### 12.2 Interval attributes

`{attr}_start` and `{attr}_end` hold the bounds at `T`'s datatype as the unit's first instant; `{attr}_start_precision`
and `{attr}_end_precision` hold the precision as a `ugdb:precision` literal (`millennium` … `nanosecond`) when
coarser than `T`'s; an open end stores no `{attr}_end` and an open start no `{attr}_start`. `@index(interval)` stores two range entries, the
outer extent's bounds (section 5.5), in the value-index keyspace under tokens derived as `range` indexes derive them.

### 12.3 Relations

An edge-encoded instance is the quad `(graph, anchor, nav predicate, other)` with, when allocated, its statement record
holding the reifier and the attributes; a role naming the instance holds the reifier, a statement ID with the
relation's prefix, as its object term, and the reifier takes the anchor's bucket as every statement takes its
subject's. A node-encoded instance is an entity-like subject of the relation's prefix,
named by its handle IRI, with one quad per role triple and its attributes as its own quads. In both, the instance's
bucket is the anchor's, so every entry of it is in one partition. A node-encoded `@set` relation keeps, in its
partition's marks keyspace, a combination key per live instance: XXH3-64 of the relation's prefix and the participants'
IDs in role order, mapped to the instance's ID, which an insert reads to find the existing instance; an edge-encoded
`@set` relation's combination is its triple; a `@symmetric` relation's participants enter the key in ID order. A
participant in another partition is registered at
its home before the commit (`../binary_protocol/PROTOCOL.md` section 11.12), in get-only mode (section 15), which answers its status, absent, live, retracted or deleted, and its
type prefix.

### 12.4 Bitemporal and derived state

`asserted` is stored as `asserted_start_epoch` and `asserted_end_epoch`, `xsd:unsignedLong` quads on the instance,
converted to `xsd:dateTimeStamp` on read from the sequencer's close times; `live` is the absence of the end. A tombstone
is a statement record at the entity's home. A materialized derivation stores its result as quads under the derived
name's predicate, written in the same commit as the write that changes it, within the anchor partition. Count indexes
serve cardinality checks, and a `@limit`'s aggregates, one record per group in the marks keyspace, serve its checks
(section 8.6). `@acyclic` is checked by walking the relation's paths from the inserted instance, bounded by
`max_depth` and by `rule_read_max`, and `@no_overlap` by reading the group's live instances within `rule_read_max`,
each relation instance a check visits counted once and a check that would visit more refused before it does
(section 8.2).

## 13. Schema evolution

**[stage 9.5]** Every change to an installed schema is built at stage 9.5, with the migrations of
`SCHEMA-EVOLUTION.md`: a change takes effect at an epoch in every partition holding the tenant, a tightening scans
them and a migration moves data between them, so each needs the cluster stage 8 builds. Until then a tenant takes one
Install; a later one is refused naming the stage, and a Drop of everything in the tenant returns its catalog to the
initial one (section 6.1).

### 13.1 Versions

Every change to a tenant's catalog is a new catalog version built on the current one, committed as pending and
effective at an epoch (`../binary_protocol/PROTOCOL.md` section 11.13). A version built on one no longer current is refused (`UGS2001`).
Queries and transactions compile against the version in effect at their snapshot or commit epoch, so no transaction
sees two versions; a read-write transaction that executed against another version than the one in effect at its commit
aborts (`PROTOCOL.md`, Conflict kind Catalog). Versions roll forward only: a later version restores an earlier
definition; nothing is rolled back in place.

### 13.2 Kinds of change

| Change | Kind | Takes effect |
|---|---|---|
| Add a type, an optional attribute, a relation, a role that is `Optional` or set-valued, an index, a derivation, a function, a graph, an enum member, a unit, a module | Additive | Online; no validation |
| Add a required attribute with a default | Additive | Online; the default applies to existing instances on read |
| Add a required attribute without a default, a key to a keyless type, `@unique`, a cardinality bound, `@acyclic`, `max_depth`, `@limit`, `@no_overlap`, `@append_only`, `@immutable`, a value check, `restrict`; narrow a role's type; remove an enum member | Tightening | After a validation scan (section 13.3); refused if existing data violates it (`UGS2003`) |
| Remove a constraint, widen a bound, make a required attribute optional, add a default, loosen `restrict` to `cascade`, add a union member | Loosening | Online |
| Change `@colocate`, a graph's placement | Relocation | Background data movement; the old placement serves reads until the move completes; the report states how much data moves |
| Remove a type, attribute, role, relation, graph or function; change an attribute's type, a key, a relation's `@set` or `@multi`, `@symmetric`, a relation's encoding, an `@iri` template, a vocabulary base | Destructive | Either a migration, which carries the data into a new version of the namespace (`SCHEMA-EVOLUTION.md`), or the deprecate-and-drop path: deprecate first (`@deprecated` on the item, additive); drop in a later version after a finalize step; the drop is refused while live instances or catalog references name the item (`UGS2002`, `UGS2009`) |

A destructive change that drops a type retires its prefix: existing IDs of it may linger in logs and are never reissued.
Dropping a predicate drops its data as `../binary_protocol/PROTOCOL.md` section 6.5 states.

### 13.3 The validation scan

A tightening change takes effect at its epoch like any version, and each partition holding the tenant then scans
its data at the epoch's first version for a contradiction, as `../binary_protocol/PROTOCOL.md` section 11.13 has every partition check a
type change. The scan reports its progress and blocks no write; writes at and after the epoch are validated by the new
version while it runs. A contradiction found in any partition refuses the install's reply with the instance at fault
(`UGS2003`), and the metadata group commits a version restoring the previous definition, so the refused change alters
no data.

### 13.4 Type prefixes and modules across versions

Prefixes are stable across every version (section 7.1). A module's upgrade is a version like any other; a module's
removal is destructive. A version that adds a subtype under an existing type extends the supertype's prefix set by one
range, which the catalog recomputes.

### 13.5 Evolution refusals

| Number | Condition |
|---|---|
| `UGS2001` | The version does not build on the current one, or a schema's version number does not rise |
| `UGS2002` | A destructive change without the item deprecated in the previous version |
| `UGS2003` | A tightening the data contradicts; the message names the instance |
| `UGS2004` | A change of a relation's encoding, key, `@set`/`@multi` or `@symmetric` outside the destructive path |
| `UGS2005` | The tenant's type prefixes are exhausted |
| `UGS2006` | A key's attributes or template changed |
| `UGS2008` | A module installed at a version below the one held |
| `UGS2009` | A drop of a type, attribute or relation that live instances or catalog values still reference |
| `UGS2010` | A role removed or retyped outside the destructive path |
| `UGS2011` | An attribute's type changed outside the destructive path |

## 14. Staging

The language is accepted whole from stage 6. Each row states from which stage the construct's runtime rule holds, or
from which it executes; before that, the compiler records the declaration, derives views and classification from it,
and reports it as unenforced (`UGS0902`). `UGS0902` is one record per declaration that waits, an `@index(ann)` declaration, a function, a
virtual derivation; what a stage refuses at run time, a rule across partitions, is no record.

| Construct | Enforced from | What waits |
|---|---|---|
| Lexical structure, grammar, every static rule of sections 5 to 7, prefixes, keys, IRIs, the RDF view, Describe | Stage 6 | |
| Required, optional, defaults, value checks, datatype, enum, `@immutable`, type fixed, closed by default, `@open` | Stage 6 | |
| `@key`, `@unique` local to the transaction's partition | Stage 6 | A claim at another partition: stage 8 |
| Relations: roles, set-valued and optional roles, unions, `@set`, `@multi`, `@symmetric`, inheritance, cardinality on the anchor role and on another role where the participant's instances are in the transaction's partition, `@on_delete` local, `@acyclic` within a partition, `@no_overlap` and `@limit` per the anchor role, `@limit` per another role where the group's instances are in the transaction's partition, `@retracts` and `@ends` local | Stage 6 | Every cross-partition case, refused with code 23 until then: stage 8 |
| `@append_only`, `@bitemporal`, `asserted`, `retract`, `@on_retract` local | Stage 6 | |
| Entity delete with its tombstone | Stage 6 | Shares at referrers: stage 8 |
| Indexes `range`, `interval`, `geo`, `fulltext` | Stage 6 | |
| `@index(ann)` with its `metric` and `quantization`; `Vector<BF16>` storage | Stage 8 | Until then an Install accepts the declaration, records it in the catalog version and reports it (`UGS0902`), and no index is built; stage 8 builds the index from the recorded declaration with no schema change. A write of a `Vector<BF16>` value is refused naming the stage, so such an attribute stays absent; a `Vector<F32>` value is stored as a value. The write path's declaration of a vector index, in a tenant with no schema, stays refused (`../binary_protocol/PROTOCOL.md` section 11.13) |
| `Measure`, `quantity`, `unit`, `Geo` operators | Stage 6 | |
| Materialized derivations within the anchor partition, computed derivations among them | Stage 6 | Materialization across partitions: the tile stage |
| `@access` | Stage 6: recorded in the catalog version and read by stage 7's query classifier (section 9.6), with no `UGS0902` | |
| Functions and virtual derivations, executed | With the query core, stage 7 | From stage 6 each is compiled, type-checked and recorded, and reported (`UGS0902`) |
| The GQL view and SHACL export | Stage 7 | |
| Write classification's coordinated and segmented paths across partitions, the fast path's use of the request-typing facts | Stage 8 | |
| Purge | Stage 8 | |
| Schema evolution: an Install into a tenant that holds a schema, the changes in place of section 13 with the validation scan and deprecate and drop, the removal of a namespace, migrations (`SCHEMA-EVOLUTION.md`) | Stage 9.5 | Until then a tenant takes one Install, of one or more sources, schemas with the modules extending them; a later Install is refused, and a Drop of everything in the tenant returns its catalog to the initial one |

## 15. What this specification asks of the others

- **`../binary_protocol/PROTOCOL.md`.** Carried as of 2026-11-05: Install (section 8.4 there) installs sources and answers the version,
  its epoch and the report, or code 28 with the diagnostics; code 7 carries a schema refusal's number at commit.
  Carried as of 2026-11-08: Describe's record kinds for the catalog entries of section 10.2, the sources and the
  retired names, covered by the schema id (sections 8.3 and 11.7 there); `EntityDeleted`, code 30; Reference (526) in
  a get-only mode answering an entity's status and type prefix (section 13.15); the datatypes `ugdb:bfloat16vector`,
  `ugdb:measure` and `ugdb:precision` (sections 10.1, 10.4, 10.5); Purge (24), section 8.8's legal purge (section
  8.5); an entity delete's tombstone, write-set operation 8 (section 12.2); a handle IRI accepted as a name, a derived
  handle among them (section 4.4); continuation frames for Catalog and Describe, a catalog version bounded by
  `catalog_bytes_max` (sections 8.3, 11.9, 13.7); Install carrying a migration and Migrate (25) controlling its run
  (sections 8.4 and 8.6); warnings with a reply (section 4.9).
  Carried as of 2026-11-10: a tenant holding a schema refuses write-path declarations, a declared predicate's Drop and
  the operations creating untyped nodes, and holds one-frame writes to blind predicates (sections 6.5 to 6.7, 7.3,
  11.13); an Install is refused into a tenant holding ungoverned data, which a Drop of everything empties, its
  default-entry records included (sections 6.5, 8.4, 11.13); a keyed IRI routes by the key it states (sections 4.4,
  11.12); Reference's get-only mode answers a retracted participant (section 13.15); a change record's `extra` is 0,
  the preorder number gone (sections 4.6, 8.3); Operation kinds 5 and 6, an instance's delete and its retraction
  (sections 6.3, 7.3, 11.6, 12.2); a first schema's fence, code 31, and one Install per tenant before stage 9.5
  (sections 8.4, 9.4, 11.2); an Install's `@index(ann)` accepted and unenforced (section 11.13); a Drop of a tenant's data or of
  everything naming the catalog version it drops under (section 6.5); the reifier of a declared relation's instance
  named by its handle IRI (sections 4.6, 9.1, 10.2).
- **`../../openspec/notes/SECURITY.md`.** Carried as of 2026-11-08: Install takes `declare` on the tenant; a purge takes the action
  `purge` (7) at tenant scope, held by the built-in `owner` role, and is an audit event naming the principal and the
  purge epoch; a migration's control takes `declare`; the scopes are cluster, tenant, namespace, graph and predicate,
  and a tenant isolates where a namespace organises.
- **Naming, across `../binary_protocol/PROTOCOL.md`, `SECURITY.md` and the stage 5.5 specifications.** Carried as of 2026-11-05: the u64
  is the tenant, the field, the bucket run, the catalog, the drops and the grants keeping their bytes and taking the
  name `tenant`, and `Bucket::derive` and `Bucket::of_value` take a tenant. The word namespace is free for the unit
  this file defines, so that a client reads PostgreSQL's distinction, database and schema, in the
  engine's own terms.
- **The capability specifications.** Stage 6's change set, which builds UGS, carries the following deltas into them,
  as an OpenSpec change applies its deltas when it archives; stage 9.5's change set carries the migration's:
  - `system-identity`: the canonical key encoding of section 7.2 for composite and non-IRI keys, as a revision of
    "Until UGS keys exist, an IRI is the key of the Resource type"; a `@key`'s claim bucket hashed from the key space's
    prefix and the canonical key, as the entity's is, with `Bucket::of_value` kept for `@unique` values; at stage 9.5,
    key spaces carried through migrations and derived handles for secondary images (section 7.3).
  - `catalog`, `schema-changes`, `schema-storage`: the catalog version's encoding of section 10.2 in place of the
    three-kind encoding, its retired-name and migration entries among it; type prefixes allocated in declaration order
    in place of preorder numbers; at stage 9.5, the validation scan of section 13.3 generalising the type change check.
  - `mapping-contract`, `term-dictionary`: relation encodings of section 11.1, with a statement ID allocated also for an
    instance that fills a role, and a reifier accepted as a role's object term; `rdf:type` answered from prefixes for
    declared types; `Iri` and enum values as literals; a handle IRI minted into its type's space, held in the
    dictionary both ways as a reifier's skolem IRI is, and presented as an IRI; the contract itself stated as
    `SCHEMA-APG.md`'s definitions 17.1 and 18.1; at stage 9.5, birth IRIs held both ways, dictionary entries
    rewritten at a cutover and versioned so a read at an earlier epoch sees the earlier entry, and forward entries for
    images a migration moves.
  - `metadata-group`, at stage 9.5: a migration run's record and its transitions, and the retired names per namespace.
- **The UQL specification.** The expression grammar of section 4 is its base; the bitemporal filter items, the interval
  operators and `schema.` introspection are named here and defined there.

## 16. Worked example

The aviation schema, with what the compiler makes of it.

```
schema aviation version 1

prefix av = "https://example.org/aviation#"
import "https://schema.org/" as sdo

attribute Serial    = String @pattern("[A-Z0-9-]{6,20}")
attribute Tag       = String @index(range)
attribute Embedding = Vector<F32, 768> @index(ann, metric: cosine)

enum Status { active, stored, retired }

/// Anything that can be owned, assembled and supplied.
abstract entity Asset @key(serial) {
  serial:       Serial
  commissioned: Date
  status:       Status
  tags:         Set<Tag>
  notes:        Text Optional @index(fulltext)
  embedding:    Embedding Optional
}
entity Aircraft : Asset { tail_number: String @unique }
entity Engine   : Asset { thrust_kn: Float64 @range(0, 2000) }
entity Avionics : Asset { firmware: String Optional }

abstract entity Organization @key(name) {
  name: String @maps_to(sdo:legalName)
}
entity Company   : Organization
entity Authority : Organization { jurisdiction: String @values("EU", "US", "UK") }

relation Owns @colocate(asset) {
  owner: Organization -> assets*
  asset: Asset        -> owner?
  since: Date
}

relation PartOf @acyclic(max_depth: 16) {
  part:  Asset -> assembly?
  whole: Asset -> parts*
} @on_delete(whole: restrict)

abstract relation Agreement @multi {
  party:        Organization -> agreements*
  counterparty: Organization -> counterparty_agreements*
  signed:       Date
}

relation Supply : Agreement @append_only {
  supplier: Company as party        -> sales*
  buyer:    Company as counterparty -> purchases*
  item:     Asset                   -> contracts*
  value:    Decimal(18, 2) @check(value > 0)
} @limit(sum(value) <= 50_000_000 per buyer)
  @on_delete(supplier: restrict, buyer: restrict, item: restrict)

relation Lease : Agreement {
  lessor: Organization as party        -> leases_out*
  lessee: Organization as counterparty -> leases_in*
  asset:  Aircraft                     -> leases*
  term:   Interval<Date>
}

relation Certifies @colocate(contract) {
  authority: Authority -> certifications*
  contract:  Supply    -> certification?
  valid:     Interval<Date>
}

derive Asset.all_parts   = parts+
derive Company.suppliers = purchases.supplier

fn Aircraft.engines() -> Engine* = all_parts(is Engine)
fn Company.spend(since: Date) -> Decimal(18, 2) = sum(purchases(signed >= since).value)

graph production : aviation @default
```

**Type prefixes**, allocated in declaration order from 3 on the first install into an empty tenant, and prefix sets:

| Type | Prefix | Prefix set |
|---|---|---|
| `Asset` | 3 | 3–6 |
| `Aircraft`, `Engine`, `Avionics` | 4, 5, 6 | each its own |
| `Organization` | 7 | 7–9 |
| `Company`, `Authority` | 8, 9 | |
| `Owns` | 10 | |
| `PartOf` | 11 | |
| `Agreement` | 12 | 12–14 |
| `Supply`, `Lease` | 13, 14 | |
| `Certifies` | 15 | |

**Encodings and anchors.** `Owns`: edge, anchor `asset`, triple `asset av:owner organization`, `av:assets` its inverse;
`PartOf`: edge, anchor `part`, triple `part av:assembly whole`, `av:parts` its inverse; `Agreement`, `Supply`, `Lease`: node (three roles), anchor `party` (`supplier`, `lessor`), roles
`av:Supply_supplier`, `av:Supply_buyer`, `av:Supply_item`, each `rdfs:subPropertyOf` the `Agreement` role it
specializes; `Certifies`: edge, anchor `contract`, triple `supply av:certification authority`.

**Predicates:** `av:serial` (unique within `Asset`'s tree, looked up by value through the reverse list), `av:commissioned`,
`av:status`, `av:tags` (one triple per member, `exact`), `av:notes` (`fulltext`), `av:embedding` (`hnsw`, cosine,
dimension 768), `av:tail_number` (unique), `av:thrust_kn`, `av:firmware`, `av:name` (unique within `Organization`'s
tree, `owl:equivalentProperty sdo:legalName`), `av:jurisdiction`, `av:since`, `av:signed`, `av:value`, `av:term_start`,
`av:term_end` and their precisions, `av:valid_start`, `av:valid_end` and theirs, the navigation predicates `av:owner`
and `av:certification`, and the role predicates above.

**IRIs:** `https://example.org/aviation#Asset/EN-4471-B` for an engine, `…#Organization/Leonardo` for a company, and
`…#Supply/3f2a1c-…` for a supply, by its handle.

**Classification**, the report's table:

| Write | Constraint | Path |
|---|---|---|
| Create `Engine`, `Company` | Key uniqueness | Segmented, local: the key's claim is in the entity's own bucket, which the creating partition owns |
| Update `status`, `notes`, `commissioned` | None beyond the instance | Coordination-free |
| Insert `Owns` | `owner?` on the asset | Segmented, local: anchored at the asset |
| Insert `PartOf` | `assembly?` on the part; `@acyclic` | Segmented, local for the bound: anchored at the part; the paths `@acyclic` walks are local when one partition holds them and coordinated otherwise |
| Insert `Supply` | `@limit` per buyer, anchored at the supplier | Coordinated; warning `UGS0904` |
| Insert `Certifies` | `certification?` on the contract | Segmented, local: anchored at the contract |
| Delete `Asset` | `restrict` on `PartOf.whole` and `Supply.item` | Coordinated for instances in other partitions; warning `UGS0903` suggests `@colocate(whole)` and `@colocate(item)` |
| Delete `Company` | `restrict` on `Supply.supplier` and `Supply.buyer` | Segmented, local for the supplier, the anchor; coordinated for the buyer's instances in other partitions; warning `UGS0903` suggests `@colocate(buyer)` |
| Indexes | Derived data within the instance's partition | Coordination-free |

**Scatter reads** (`UGS0905`), every navigation of a role that is not its relation's anchor (section 9.1), and no
other: `Organization.assets`, `Asset.parts`, `Organization.counterparty_agreements`, `Company.purchases`,
`Asset.contracts`, `Organization.leases_in`, `Aircraft.leases` and `Authority.certifications`; and the derivations
that begin with one, `Asset.all_parts` and `Company.suppliers`.

## 17. Decisions

| Question | Decision |
|---|---|
| The system ID's layout | Type 16, sequence 64, bucket 24, reserved 24, in that order, as the code and the `system-identity` specification hold it; the note's decisions table, which says bucket 16 and reserved 32, is superseded by its own text and P24 |
| Type prefixes | Allocated in declaration order from 3, never renumbered; a supertype is a set of ranges (D2) |
| Attribute identity | One predicate per attribute name per vocabulary base, shared by every type declaring the name with agreeing declarations; `@key` and `@unique` attributes belong to one tree. Chosen so the vocabulary term is `av:serial`, as SPARQL clients write it, and an inherited attribute is one predicate; the note's "one predicate per declaring type" is kept as one catalog entry per declaring type |
| Closed by default against the store's default entries | A governed tenant refuses undeclared data unless `@open` on the schema admits `Resource` subjects or `@open` on a type admits undeclared attributes; both record default entries as the store does today |
| Indexes and materialized derivations | Written in the instance's partition in its local commit, as P24 holds for every derived entry; `@materialized` is bounded to derivations within the anchor partition, computed derivations among them, until the tile stage. The note's change streams belong to that stage |
| Relation encodings | Edge for binary, single-valued relations without `@key`, their instances filling roles through their reifier; node otherwise. Fixed by the roles' shape, the placement and the key, never by attributes or by being named, so neither re-encodes. A keyed instance takes a bucket that is no participant's, and an edge whose subject lives elsewhere would give one fact two owners. Edge encoding through the reifier: 2026-11-05, the Gen5 exercise's G3, two quads fewer per instance for most of its relation kinds |
| A subrelation's anchor | A subrelation may declare its own `@colocate`; inherited constraints anchored elsewhere become non-anchor constraints for that kind, which the report states (the exercise's G1): DeepBrain's `Hierarchy` and `LiveUnit` need it |
| A keyed relation's home | Its key's bucket, as an entity's, and no `@colocate`: one rule for where an instance lives, derivable from its IRI (the exercise's G2) |
| Index writes in the report | Counted per write operation (the exercise's G4) |
| Relation node of an edge-encoded relation | Decided 2026-11-05: an edge-encoded relation whose instances carry a reifier is presented in the GQL view also as a relation node with derived role edges, one element with the edge and one `ELEMENT_ID`, so a relation can name it, as `AssertedBy` names every relation of Gen5; the counterpart of the RDF view's reifier term beside its triple. Chosen over a node-only presentation, which leaves the store unchanged and turns every hop of Gen5 into three atoms or a propertyless navigation edge, losing the envelope on the hop |
| Retraction and non-bitemporal relations | Decided 2026-11-05: a retraction cascades to `@bitemporal` relations alone; a relation without an `asserted`, an `@append_only` link above all, is live while every participant of it is live, derived and never written, so provenance of a retracted statement stays as written and returns under `with retracted`. Chosen over writing the link, which `UGS1013` forbids, and over leaving it in the live view, which would show an edge to a node the view does not show |
| Navigation edges | A node-encoded relation's navigation names are derived one-hop edges in both views, read-only, so a quantified edge or a property path walks a relation node as it walks an edge; what makes DeepBrain's tree walks expressible in the query cores (`../query/QUERY-GQL-CORE.md`, `query/QUERY-SPARQL-CORE.md`, 2026-11-05) |
| Tenant and namespace | Decided 2026-11-05: a tenant isolates, with nothing read, written or named across tenants; a namespace organises inside a tenant, one `schema` with its graphs, as a PostgreSQL schema inside a database. The engine's u64 is the tenant; the UGS declaration of a vocabulary base is `prefix`, since `namespace` now names the unit |
| `rdf:type` of declared types | Answered from the type prefix; no stored quad |
| `Iri` and enum values | Literals (`xsd:anyURI`, `xsd:string`), so a value creates no entity, no referrer set and no registration; the note's "IRI term" and "one IRI per member" are kept as vocabulary, not as object terms |
| `DateTime` and `LocalDateTime` | `xsd:dateTimeStamp` and `xsd:dateTime` without a timezone, which keeps the offset requirement in the datatype |
| Interval precision | Stored as the unit's first instant plus a precision predicate per bound. Decided 2026-11-08: the outer extent alone is indexed, and a `certainly` form filters its candidates, which halves the interval index |
| Named attribute types | A reusable declaration whose values are literals. Decided 2026-11-08: every value is found through the reverse lists and ordered by `@index(range)`, so no value becomes an entity; a value with relations of its own is an entity, a `Concept` |
| The `schema` prefix | Reserved for the catalog qualifier; the example imports schema.org as `sdo` |
| Keyless names | A handle, the bucket and 104 random bits, minted into the type's IRI space, held in the dictionary both ways as a reifier's skolem IRI already is, and presented as an IRI. Chosen over D6's keyed encryption, which puts a key service on the read path of every keyless IRI and ties each such IRI to key retention, at the price of two dictionary entries the design already pays per reifier; revisited only if stage 7's dictionary-growth measurement shows those entries dominating |
| Graphs and bases | One base per graph, defaulting to the schema's; two graphs of one schema take two bases, so IRIs stay unique in a tenant |
| Cardinality markers | Always written; a relation cardinality is a check that refuses, never a replacing store cardinality |
| Decimal bounds | `p ≤ 38`, `s ≤ p`, `s ≤ 18`, `p − s ≤ 20`, from the store's i128 × 10¹⁸ encoding |
| Cross-partition paths | Specified here, enforced from stage 8 |
| Version 1 aggregates in `@limit` | `count` and `sum` |
| `Measure` representation | `xsd:double` only; money is a `Decimal` attribute beside a currency enum, outside `quantity`, since currencies convert by rates that vary in time |
| `@values` | A value check per declaring type, exported as `sh:in` per shape |
| Purge | A command over a bounded, reviewed list of IRIs under the action `purge`, sequenced at an epoch and executed as a delete's shares with physical removal everywhere the instance is named; the audit record holds each IRI's hash, never the IRI |
| HNSW parameters | Version 1 declares `metric` and `quantization`; build parameters take the store's defaults and are added as optional arguments when stage 8's measurements ask; a search-time parameter is refused in a schema |
| Spread `@limit` bounds | Decided 2026-11-08: coordinated, as every spread constraint is; DeepBrain's only spread bound, one preferred instance, is a lease that coordination already gives |
| Reserved words | The structural keywords only; every operator, aggregate, modifier and annotation word is contextual, so domain names such as `in`, `by`, `max`, `open` and `within` stay available (found by the Gen5 exercise, `UltraGraphDB Schema Feedback.md`). Decided 2026-11-08: `import`, `unit` and `version` are keywords only where a header or declaration begins, which lets section 5.11's `.unit` parse and Gen5 keep `version`, `unit` and `import` as names (`deepbrain-gen5.ugs`) |
| Computed derivations | Decided 2026-11-08: a derivation may be an expression over the instance's own fields and built-in fields, maintained in the instance's own commit and indexable, which is how Gen5's `closed` flag lives with the statement it describes |
| `@limit` conditions over participants' types | Decided 2026-11-08: a top-level conjunct reading only the grouped participants' types selects the groups a bound applies to, so a lower bound can hold for one type of participant, as Gen5's `GeneratedBy` and `ActedOnBehalfOf` require |
| Catalog types as field types | Decided 2026-11-08: the grammar's `type` takes `schema.EntityType<T>` and `schema.RelationType<T>`, as section 5.10 lets a field hold one |
| Unit names | Decided 2026-11-08: a unit is written as its symbol, UCUM's case kept, outside the `UpperCamelCase` convention |
| Name spaces | Quantity kinds and units have their own, resolved by position, so a kind may be named as a scalar type is |
| Key claims | A `@key`'s claim is in the entity's own bucket, hashed as the entity is, so creation and the check meet in one partition; `@unique` claims stay at the value's hash |
| `@ends(role)` | Beside `@retracts`: a succession closes the old statement's valid interval and leaves it believed |
| Symmetric relations | Stored in ID order under either encoding; the navigation reaches the instance from both participants |
| Attribute predicates | `@iri` on an attribute splits a shared name into a predicate of its own |
| Quantifiers | `all` beside `some` and `no`, over a path with a filter, in place of a lambda |
| Trees | Decided 2026-11-08: `@acyclic` walks the relation's paths from an insert, coordinated where they cross partitions, and no placement keeps a tree in one partition. DeepBrain's organisation tree changes rarely and its reads walk it; access by organisation unit, which may read it on every request, is decided with relationship-based access control (`../../openspec/notes/SECURITY.md`) |
| Graphs | Decided 2026-11-08: a graph is a named instance of a schema, and a relation reaches every graph of the schemas its roles name, one rule within and across namespaces |
| Derivations | Decided 2026-11-08: paths, closures and computed values; an aggregate over a path is a function, computed when read |
| A position's uncertainty | Decided 2026-11-08: an attribute of its own, which a query adds to its radius; whether a position certainly lies in a region is DeepCausality's reasoning, as counterfactuals are |
| Set indexes, role names, predicate claims, unit mappings | Decided 2026-11-08: an index on a `Set<T>` indexes each member; a role takes no built-in field's name (`UGS0417`); `UGS0216` holds per predicate, so `@iri` takes a declaration outside another tree's claim; `@maps_to` on a quantity kind or a unit is `skos:exactMatch` |
| Claims of history-keeping types | Decided 2026-11-08: a retracted instance keeps its key's and its values' claims, so an IRI names one instance for all time and no identity is reissued, as deep_brain's `IdAllocator` requires (section 7.2) |
| Conformance corpus | Built once the language is final for DeepBrain's current target; the proposal is section E of `openspec/notes/UltraGraphDB Schema Feedback.md` |
| Multiple inheritance | None; union roles cover overlapping participant sets |
| A standalone compiler binary | None; the server compiles, in CI through the development container |
| Formal semantics | Decided 2026-11-08: an algebraic property graph is the foundation, the GQL view a closed graph type read with PG-Schema's STRICT, closed semantics, constraints with PG-Keys' semantics, and the RDF view RDF 1.2 with reifiers and SHACL (`SCHEMA-APG.md`, sections 10 and 21). Chosen over PG-Schema as the foundation, whose edges join two nodes and never an edge, whose types are structural rather than nominal, and which has no schema mappings |
| `restrict open` | Decided 2026-11-08: refuses a retraction while open instances name the participant and retracts the live ones no longer open with it, since a statement about a participant the graph no longer believes in cannot stay believed; their history stays readable `believed at` an earlier instant |
| Role edge labels | Decided 2026-11-08: every role edge of the GQL view, a node-encoded relation's and a relation node's derived ones alike, is labelled `{Relation}_{role}`, the RDF role predicate's local name, and for a specialized role the super role's too (section 11.3; `SCHEMA-APG.md`, definition 18.1) |
| `@retracts` roles | Decided 2026-11-08: a reference into history, outside `@on_retract`, so a correction stays live naming the statement it retracted, as Gen5's `Supersedes` needs; a read following the role reaches the participant as `with retracted` reads it, and the slices are closed under every other role (section 8.4, rule 7; `SCHEMA-APG.md`, proposition 15.2) |
| Participants of `@bitemporal` relations | Decided 2026-11-08: `@bitemporal` types or `@append_only` entity types alone (`UGS0602`), as `@retracts` already requires of its role (`UGS0512`): a deleted participant would leave every version of the relation naming an instance that is gone, and a relation that is not `@bitemporal` leaves the live slice with its own participant |
| Key spaces and birth IRIs | Decided 2026-11-08: a key's bucket, claim and IRI read its key space, which outlives renames and splits, and an instance keeps the IRI it was minted with through every migration, so every IRI ever minted routes without a lookup (sections 7.2 and 7.3; `SCHEMA-EVOLUTION.md` section 4) |
| Retired names | Decided 2026-11-08: the catalog keeps every name an item held, for routing and for the hint after a deprecation window, and never gives one to another item (section 10.2) |
| GQL view's node types | Decided 2026-11-08: one node type per concrete type with its exact label set, supertypes as labels only, since a closed supertype admits no subtype's node |
| Refusal numbers and revisions | Decided 2026-11-10: a refusal number names the rule broken, the same for every mutability class, so a client acts on a number without knowing the type's class and each constraint of `SCHEMA-APG.md` section 14 raises one refusal. A revision is how an update of a `@bitemporal` instance is stored, never a rule: its refusals are the rows' own, `UGS1007`, `UGS1012` and `UGS1013`, as sections 8.2, 8.3 and 8.4 state them. `UGS1025`, which had named all of them for `@bitemporal` types alone, keeps the one condition it named that no other number carried, a required role's participant fixed for the instance's life, and names it for every relation (RoleFixed) |
| Subjects at creation | Decided 2026-11-10: an instance is created under the IRI its type mints and no other (`UGS1038`), since every node routes an IRI by the catalog alone and one write has one spelling; the key is never filled in from the IRI, where a typing error would create an instance in place of a refusal |
| A governed tenant's catalog | Decided 2026-11-10: the compilation of its sources and nothing else, as Describe's sources and the schema id present it: write-path declarations and a declared predicate's Drop are refused, a first schema installs into a tenant holding no ungoverned data, and one-frame writes carry blind predicates alone, keyed creation taking a transaction until the query core's one-statement requests |
| Derived triples | Decided 2026-11-10: a triple the view derives is never written or deleted (`UGS1029`), so one fact has one stored form, owned by its subject's partition, and `R` of `SCHEMA-APG.md` definition 17.1 round-trips. Chosen over normalising an inverse to its stored triple, which binds the transaction to the wrong subject's partition |
| Reifiers of `@bitemporal` edges | Decided 2026-11-10: a `@bitemporal` edge-encoded instance always holds a reifier, whose statement record carries `asserted` and by whose IRI it is retracted. Allocation otherwise stays per instance, so an edge without a reifier has no `iri`, is deleted by its quad and is never retracted |
| Later-stage constructs in an Install | Decided 2026-11-08, applied 2026-11-10: an Install accepts a construct of a later stage, `@index(ann)` first, records it and reports it (`UGS0902`); the stage that executes it builds from the recorded declaration with no schema change. Chosen over refusing the Install, which refuses Gen5 for one line of 975, and over stripping the line, which tests another schema |
| Evolution's stage | Decided 2026-11-10: every change to an installed schema is stage 9.5's, in place or by migration, since each takes effect across every partition holding the tenant; until then a tenant takes one Install, fenced from its check to its effective epoch so no data is written ungoverned, and is emptied by a Drop of everything |
| An instance's delete and retraction | Decided 2026-11-10: commit-time operations naming the instance (`PROTOCOL.md` section 7.3), decided in commit order against the partition's state, to which every query language compiles; a change record names a statement, and these name an instance |
| The catalog version's bytes | Decided 2026-11-10: specified before they are implemented, in the protocol's body layout, each entry self-contained, checksummed and verified when read, indexed by type prefix for the commit, with the expressions a commit evaluates as verified postfix programs (section 10.2, `CATALOG.md`). Chosen over an encoding left to its first implementation, which every role, every later stage and every client in another language would then have to read out of the code |
| Evaluation of expressions | Decided 2026-11-10: one evaluation for the schema and the query, two-valued, an attribute's check holding vacuously of an absent value, and failing closed, so no write passes on an expression that could not be evaluated (section 4) |
| The report | Decided 2026-11-10: records of the catalog version, rendered deterministically for the Install's reply, so a test and a client compare data and the text is never parsed (section 9.5) |
| Drops and history | Decided 2026-11-10: the retention of `@bitemporal` and `@append_only` instances binds instance operations; a Drop removes a container with its history, names the catalog version it drops under and is audited (section 8.4). Until stage 9.5 a Drop of everything is how a tenant's schema is changed, so the one irreversible operation is explicit and cannot be aimed at another schema's data |
| A reifier's name | Decided 2026-11-10: an edge-encoded instance of a declared relation is named by the handle IRI of its type, presented as an IRI, since an instance has one name; a skolem IRI names the reifier of a statement of no declared relation |
| `statement` as a word | Decided 2026-11-10: a keyword after `abstract` alone and an identifier elsewhere, as `import`, `unit` and `version` are placed, so Gen5 keeps `statement` as the role its provenance links name; a name differing from a reserved word by case alone is a name, and the parser's refusal names a mistyped keyword |
| One condition, one number | Decided 2026-11-10: the most specific diagnostic naming a condition is raised, `UGS0710` is retired into `UGS0116`, a commit reports the first rule broken in section 8.2's order, and an expression that cannot be evaluated has a number of its own (`UGS1039`), apart from a check that evaluated and did not hold |
| Creation | Decided 2026-11-10: a transaction creates an instance by writing `rdf:type` on a blank node or on a keyed type's minted IRI; a creation on a key the tree holds is a duplicate (`UGS1008`) and never an update, and a keyed instance under a blank node is refused (`UGS1038`), the IRI being a pure function every client computes. Chosen over minting for the client, which has the frontend assemble an instance to route it |
| One-frame writes | Decided 2026-11-10: a rule over the instance's own fields is validated in a DirectWrite's one turn; a claim, and a rule reading beyond the instance, take a transaction (section 9.6) |
| A recreated IRI | Decided 2026-11-10: the new instance's record names the ID it replaced and the tombstone the delete's version, so a read of the past never answers absence where an instance was; the dictionary stays without versions (section 8.5) |
| Order of sources | Decided 2026-11-10: schemas by name, then modules after what they extend, so prefix allocation is a function of the sources alone (section 7.1) |
| What a rule reads at commit | Decided 2026-11-10: an update of an attribute a rule's condition reads is checked as an update of what it aggregates; a `@limit` keeps its aggregate per group, one read and one write whatever the group's size; `@acyclic` and `@no_overlap` read within `rule_read_max` and refuse past it (`UGS1040`); `@acyclic` without `max_depth` stays legal under the same bound (sections 8.2, 8.6, 9.4) |
| Declared predicates' IDs | Decided 2026-11-10: allocated by the metadata group with the version, the `Resource` prefix with sequences from 2^63, held in the version's predicate index and never in an entry, since Describe carries entries and a system ID never leaves the database; a governed tenant resolves no declared predicate through the dictionary (section 7.1, `CATALOG.md` section 2) |
| Where an install compiles | Decided 2026-11-10: beside the metadata group, from the base version and the next free prefix and predicate sequence, so the group's thread only commits bytes; the version is decoded and encoded back before it is committed, and a failure is the compiler's, refused with code 7 (section 10.1) |
| Corpus readings | Decided 2026-11-10: where the conformance corpus, written from this file alone, found it silent, ambiguous or contradicting itself, the reading decided is written in. A relation's anchor is section 9.1's, never the compiler's choice, and `UGS0904` names a pair of competing constraints; `UGS0903` is `restrict`'s alone; a commit reports the first rule broken in an order that takes a value's type before a check reading it and the more specific rule first, with rows for `UGS1026`, `UGS1034`, `UGS1036` and `UGS1037`; a tombstoned subject is code 30 and a tombstoned participant `UGS1019`; 65,532 declared types, so a prefix and a range's end fit 16 bits; `max_depth` and `rule_read_max` count relation instances; locations, number tokens, negative literals in arguments and defaults, a required `prefix`, `schema` reserved, qualified enum extensions in modules, attribute identity across inheritance, and the codes of undeclared participants, nested union members, set-valued and append-only `orphan` and a specialized role's shape (sections 3.1, 3.4, 3.5, 3.7, 4, 5.1, 5.4, 5.5, 6.1, 6.3 to 6.7, 6.10, 6.12, 7.1, 7.4, 8.1 to 8.3, 8.5, 8.7, 9.1, 9.3 to 9.5, 10.2, 10.4, 11.2, 12.1, 12.4, 14) |

## 18. Sources

- `openspec/notes/UltraGraphDB Schema and Query Language.md`, the design this file specifies
- `openspec/notes/UltraGraphDB Distribution Architecture.md`, the coordination model, write classification and query
  consistency levels
- `openspec/notes/UltraGraphDB Distribution Protocol.md`, the fast path's qualification from the classification
- `openspec/notes/UltraGraphDB Shard-per-Core Write Path.md`, P1, P3, P7, P13–P16 and P24
- `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`, the quad layout, datatypes by value, the data dictionary
- `../../openspec/notes/review-stages-5.5-and-6.md`, D1–D6 and the findings on the note
- `../binary_protocol/PROTOCOL.md`, sections 1, 4.6, 6.5, 8.3, 10.4, 10.5, 11.2a, 11.7, 11.11–11.13, 13.7
- `openspec/specs/`: `system-identity`, `metadata-group`, `catalog`, `term-dictionary`, `mapping-contract`,
  `schema-changes`, `schema-storage`, `vector-index`, `tokenizers`
- RDF 1.2 Concepts, SPARQL 1.1, SHACL, OWL 2, GeoSPARQL 1.1, QUDT, ISO/IEC 39075 GQL, XML Schema 1.1 Part 2, UCUM,
  RFC 3986, BCP 47
- Bailis et al., *Coordination Avoidance in Database Systems* (VLDB 2015); Whittaker and Hellerstein, *Interactive
  Checks for Coordination Avoidance* (VLDB 2019); Lesani et al., *Hamsaz* (POPL 2019); Roy et al., *The Homeostasis
  Protocol* (SIGMOD 2015); Power et al., *Keep CALM and CRDT On* (VLDB 2023)
- `SCHEMA-APG.md`, the formal model the standard views present
- Shinavier, Wisnesky and Meyers, *Algebraic Property Graphs*, arXiv:1909.04881; Angles et al., *PG-Schema: Schemas for
  Property Graphs* (SIGMOD 2023) and *PG-Keys: Keys for Property Graphs* (SIGMOD 2021); Ahmetaj et al., *Common
  Foundations for SHACL, ShEx, and PG-Schema*, arXiv:2502.01295; Thimmaiah, Lin and Gligoric, *MGQL: An Executable,
  Small-Step Semantics of GQL* (OOPSLA 2026)
