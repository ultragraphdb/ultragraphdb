# The catalog version's bytes

* Status: first draft, 2026-11-10, format 1, for stage 6 (`../../ROADMAP.md`).

This file specifies the bytes of a catalog version: the compiled form of every schema and module installed in a
tenant, which the metadata group holds, every role decodes, and Describe carries to clients entry by entry.
`SCHEMA.md` section 10.2 says what each entry holds and gives the six rules this layout keeps; this file gives the
layout. `../binary_protocol/PROTOCOL.md` carries a version's bytes opaque (its sections 8.3 and 13.7) and lends this
file its conventions, its body layout and its string list, so one reading discipline serves the wire and the catalog.

This file leads the code: `ultragraphdb_core_catalog` implements it, and a client in any language reads Describe's
entries from it.

## Contents

1. Conventions
2. The version
3. An entry and its shared structures
4. Programs
5. The entry kinds
6. One encoding: what a decoder checks
7. What a role holds decoded
8. Worked example
9. Formats
10. Decisions

## 1. Conventions

`PROTOCOL.md` section 1 holds: little-endian integers, read byte by byte with no alignment assumed in memory; every
padding, reserved and unused byte zero; text UTF-8 by offsets and lengths; `ABSENT`, the u32 `0xFFFFFFFF`, for no
string, no constant and no program; a Boolean a u8 of 0 or 1; XXH3-64 as the xxHash specification gives it.

| Term | Meaning |
|---|---|
| Body | The layout of `PROTOCOL.md` section 4.1: a fixed part of `F` bytes, a multiple of 8; a section table, per section an `offset` u32 and a `len` u32 counted from the body's first byte; then the sections in order, each starting at the next multiple of 8, the bytes between zero. A body ends where its last section ends. A section of length 0 still has its offset |
| String list | `PROTOCOL.md` section 4.3: a section `ends`, one u32 per string, and a section `text`, the strings back to back |
| String | A u32 index into the string list of the body the field lies in, or `ABSENT` where the field's row says so |
| Qualified name | `{schema}.{Name}` for a type, enum, record, named attribute type, quantity, unit or graph; `{schema}.{Type}.{member}` for an attribute, role, navigation name, derivation or method function; `{schema}.{name}` for a free function. `{schema}` is the declaring schema's or module's name |
| Record | A fixed-width element of a section, whose length is a whole number of records |
| T*n* | Decision *n* of `../../openspec/notes/UltraGraphDB Scalable Tile Store.md`, section 6 |

A version has one encoding (section 6): two versions with equal content have equal bytes, and a decoder refuses
every byte sequence that is not the encoding of some version.

## 2. The version

A version is a body with a fixed part of 64 bytes and seven sections.

| Offset | Width | Field | Type | Rule |
|---|---|---|---|---|
| 0 | 8 | `checksum` | u64 | XXH3-64, seed 0, of the version's bytes from offset 8 to its end |
| 8 | 4 | `magic` | bytes | `55 47 43 56`, the ASCII of `UGCV` |
| 12 | 2 | `format` | u16 | 1 |
| 14 | 2 | zero | 0 | |
| 16 | 8 | `tenant` | u64 | the tenant the version belongs to |
| 24 | 8 | `version` | u64 | the catalog version's number, at least 1 |
| 32 | 4 | `entries` | u32 | the entry count |
| 36 | 4 | `types` | u32 | the count of Type entries |
| 40 | 2 | `prefix_next` | u16 | the tenant's next free type prefix once this version is allocated, at least 3, at most 65,535 and above every prefix a Type entry holds; prefix 65,535 is never allocated (`SCHEMA.md` section 7.1) |
| 42 | 6 | zero | 0 | |
| 48 | 8 | `predicate_next` | u64 | the tenant's next free predicate sequence once this version is allocated, at least 2^63 and above every sequence the predicate index holds |
| 56 | 4 | `unit_last` | u32 | the highest unit number the tenant has assigned once this version is allocated, 0 for none, at least every number a Unit entry holds (section 3.1) |
| 60 | 4 | `quantity_last` | u32 | the highest quantity-kind number the tenant has assigned once this version is allocated, 0 for none, at least every number a Quantity entry holds |

| Section | Contents |
|---|---|
| 0 | the entry table, 16 bytes each |
| 1 | the entries' bytes, back to back in entry order |
| 2 | the entries' names: string list `ends` |
| 3 | the entries' names: string list `text`, each distinct name once, in the order the entry table first uses it |
| 4 | the prefix index, 8 bytes each |
| 5 | the kind directory, 21 u32s |
| 6 | the predicate index, 16 bytes each |

**The entry table.** One record per entry, the record of Describe's section 7 (`PROTOCOL.md` section 8.3), so
Describe carries a version's entries and their bytes as they stand and writes only its own string indices and
offsets:

| Entry offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8: the entry's kind, 1 to 19 or 21 (section 5) |
| 1 | 3 | zero | 0 |
| 4 | 4 | `name` | u32: the name the catalog orders the entry by, a string of sections 2 and 3 |
| 8 | 4 | `offset` | u32: the entry's first byte in section 1, where the bytes of the entry before end |
| 12 | 4 | `len` | u32: its bytes' length |

Entries come by kind, then by name as UTF-8 bytes; no two entries share a kind and a name.

| Kind | Entry | Its name |
|---|---|---|
| 1, 2 | Schema, Module | the schema's or module's name |
| 3, 4, 17 | Type, Prefix ranges, Classification | the type's qualified name; one entry of each per declared type |
| 5, 7, 8, 14 | Attribute, Role, Navigation, Derivation | the member's qualified name; an Attribute entry per declaring type |
| 6 | Predicate | the predicate's IRI |
| 9 to 13, 16 | Enum, Record, Named attribute type, Quantity, Unit, Graph | the qualified name |
| 15 | Function | the qualified name |
| 18 | Report | the empty string; one entry per version |
| 19 | Source | the source's name |
| 21 | Layout | the empty string; one entry per version holding a Schema entry, none in any other |

**The prefix index.** One record per Type entry, ascending by `prefix`, so a role reaches a type from an ID's
prefix by a search over fixed-width records:

| Record offset | Width | Field | Type |
|---|---|---|---|
| 0 | 2 | `prefix` | u16: the type's prefix, at least 3 |
| 2 | 2 | zero | 0 |
| 4 | 4 | `entry` | u32: the index, in the entry table, of the Type entry holding that prefix |

**The predicate index.** One record per Predicate entry, ascending by `sequence`, so a role reaches a predicate's
entry from its system ID by a search over fixed-width records:

| Record offset | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `sequence` | u64: the sequence of the predicate's system ID, at least 2^63 |
| 8 | 4 | `entry` | u32: the index, in the entry table, of the Predicate entry |
| 12 | 4 | zero | 0 |

A declared predicate's system ID is the `Resource` type's prefix 1, this sequence, and the bucket of its IRI in the
version's tenant (`SCHEMA.md` section 7.1). The metadata group allocates the sequences with the version, from the
tenant's `predicate_next`, to new predicates in the order of their Predicate entries, and never reissues one. The ID is
held here and in no entry, because Describe carries entries to clients as they stand and a system ID never leaves the
database.

**The kind directory.** Value `k`, for `k` from 1 to 21, is the number of entries whose kind is at most `k`: the
entries of kind `k` are those from index value `k − 1`, 0 for kind 1, up to value `k`. Value 21 is `entries`. Kind 20,
the retired-name entry of a later format (section 9), holds no entry in format 1, so values 19 and 20 are equal.

## 3. An entry and its shared structures

Every entry of kinds 1 to 18 and 21 is a body. Its fixed part and its sections are its kind's (section 5), and its last two
sections are always its own string list, `ends` then `text`, which every string field of the entry indexes: an
entry names nothing through another entry's strings, so Describe carries any subset of a version's entries and the
schema id hashes each as stored. An entry of kind 19 is its source's text and nothing else.

**Strings are interned in order of first use:** the fixed part's fields in offset order, then each section's
records in order, field by field. Each distinct string appears once, and the list holds no string that no field
uses.

**References between entries are by name.** A field naming a type, an attribute, a role or a unit holds its
qualified name, and the entry of that name and kind exists in the version. A role resolves names to entries once,
when it loads the version (section 7).

**Lists keep their source order** unless a section says another: a type's fields supertypes first, then in
declaration order; union members, enum members and key attributes as written.

Four structures recur in several kinds:

### 3.1 The type table

A section of 16-byte descriptors, each the type of a value. A field holding a type holds a descriptor's index.

| Descriptor offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `tag` | u8, below |
| 1 | 3 | zero | 0 |
| 4 | 4 | `a` | u32 |
| 8 | 4 | `b` | u32 |
| 12 | 4 | `c` | u32 |

| Tag | Type | `a`, `b`, `c` |
|---|---|---|
| 1 | `Bool` | 0 |
| 2 to 6 | `Int8`, `Int16`, `Int32`, `Int64`, `Int128` | 0 |
| 7 to 10 | `UInt8`, `UInt16`, `UInt32`, `UInt64` | 0 |
| 11, 12 | `Float32`, `Float64` | 0 |
| 13 | `Decimal(p, s)` | `a` = `p`, `b` = `s` |
| 14, 15, 16 | `String`, `Text`, `Bytes` | 0 |
| 17 to 21 | `Date`, `Time`, `DateTime`, `LocalDateTime`, `Duration` | 0 |
| 22 | `Interval<T>` | `a` = the descriptor index of `T`, lower than its own |
| 23 | `Iri` | 0 |
| 24 | an enum | `a` = its qualified name, a string |
| 25, 26 | `List<T>`, `Set<T>` | `a` = the descriptor index of `T`, lower than its own |
| 27 | a record | `a` = its qualified name, a string |
| 28 | `Vector<E, n>` | `a` = 1 for `F32`, 2 for `BF16`; `b` = `n` |
| 29 | `Geo` | 0 |
| 30 | `Measure` | `a` = the quantity kind's qualified name or `ABSENT`; `b` = the unit's qualified name or `ABSENT`, and `ABSENT` when `a` is |
| 31, 32 | `Unit`, `QuantityKind` | 0 |
| 33, 34 | `schema.EntityType<T>`, `schema.RelationType<T>` | `a` = `T`'s qualified name |
| 35 | an instance of a type | `a` = the type's qualified name; a participant in a program, a function's parameter or result |
| 36 | `Id128` | 0 |
| 37 | `Precision`, an interval bound's precision (`SCHEMA.md` sections 5.5 and 12.2; T48, C30) | 0 |

A field declared with a named attribute type holds that type's underlying descriptor, its annotations merged into
the field's. A parameter a tag does not use is 0. A table holds each distinct descriptor once, in order of first use,
an inner descriptor before the one naming it.

**Unit and quantity-kind numbers.** The store holds a unit, a quantity kind and a measure's unit by number, not by
name (`../storage/STORAGE.md` section 5.2, tags 30 to 32). A unit takes its number when it is first declared in a
tenant, the tenant's `unit_last` plus one, from 1, and keeps it in every later version; a number is never reused, a
unit's removal included, as type prefixes are never reused. Quantity kinds are numbered the same way, from
`quantity_last`, in a numbering of their own. A version numbers its new units and its new quantity kinds each in the order of their entries, and the Unit and Quantity entries carry them (section 5.7).

### 3.2 Constants

A section of 8-byte records, each a literal of the schema: a default, a bound, a listed value, an operand of a
program.

| Constant offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `type` | u32: its descriptor's index |
| 4 | 4 | `text` | u32: a string, its canonical lexical form |

The lexical form is `PROTOCOL.md` section 10.5's for its datatype; an enum member's is its name; a unit's and a
quantity kind's its qualified name; a rational's `n/d` in lowest terms with `d` at least 1; an interval's its two
bounds' forms around `..`, an open bound empty. A decoder refuses a form that is not canonical for its type. A role
parses each constant once, when it loads the version. Each distinct constant appears once, in order of first use.

### 3.3 Value checks

A section of 16-byte records, the checks of an attribute, a named attribute type or a record's field:

| Check offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8: 1 `@pattern`, 2 `@range`, 3 `@values`, 4 `@check` |
| 1 | 3 | zero | 0 |
| 4 | 4 | `a` | u32: 1 the pattern, a string; 2 the minimum, a constant, or `ABSENT` for `open`; 3 the first value's index in the entry's values section; 4 the program's index |
| 8 | 4 | `b` | u32: 2 the maximum, a constant, or `ABSENT`; 3 the values' count, at least 1; 4 the check's source text, a string, which a refusal's message quotes; 0 for 1 |
| 12 | 4 | zero | 0 |

The values section is u32s, each a constant's index.

## 4. Programs

An instance check, a `@limit` condition and a computed derivation are stored compiled: a postfix program over the
instance's own fields, the built-in fields, its participants' immutable fields and types, and constants
(`SCHEMA.md` sections 4 and 6.6 to 6.8). A function's body and a path derivation's, which the query core executes
from stage 7, are stored as their text (section 5), and a later format gives them a compiled form.

An entry holding programs has a section of program records and a section of instructions.

| Program offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `first` | u32: the index of its first instruction, where the program before ends |
| 4 | 4 | `len` | u32: its instruction count, at least 1 |
| 8 | 4 | `result` | u32: the descriptor index of its result's type |
| 12 | 1 | `depth` | u8: the most values its stack holds, 1 to 64 |
| 13 | 1 | `filter` | u8 boolean: 1 a filter of a quantifier, evaluated once per element and allowed `ELEMENT` |
| 14 | 2 | zero | 0 |

| Instruction offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `op` | u8, below |
| 1 | 1 | `x` | u8: the opcode's selector, 0 where it has none |
| 2 | 2 | zero | 0 |
| 4 | 4 | `arg` | u32: the opcode's operand, 0 where it has none |

A program is evaluated left to right over a stack of values, each present or absent. `Pops` and `Pushes` give each
opcode's effect.

| Op | Name | `x`, `arg` | Pops | Pushes |
|---|---|---|---|---|
| 1 | `FIELD` | `arg`: an own attribute's name | 0 | its value, absent when unset |
| 2 | `BUILTIN` | `x`: 1 `type`, 2 `iri`, 3 `live`, 4 `open`, 5 `asserted` | 0 | the built-in field |
| 3 | `ROLE` | `arg`: a role's name | 0 | its participant, absent for an empty optional role; for a set-valued role, the collection of its participants |
| 4 | `PFIELD` | `arg`: an attribute's or a required role's name | 1, a participant | that participant's field |
| 5 | `CONST` | `arg`: a constant's index | 0 | the constant |
| 6 | `ACCESS` | `x`: 1 `value`, 2 `unit`, 3 `start`, 4 `end`, 5 `start_precision`, 6 `end_precision`, 7 `quantities`, 8 `ucum`, 9 `symbol`, 10 `multiplier`, 11 `offset`, 12 `dimension`, 13 `scale`, 14 `canonical`, 15 `type` of a participant | 1 | the accessor's value (`SCHEMA.md` section 5.11) |
| 7 | `RECFIELD` | `arg`: a record field's name | 1, a record | the field |
| 8 | `ELEMENT` | none | 0 | the element a quantifier's filter is evaluated for |
| 16, 17, 18 | `NOT`, `AND`, `OR` | none | 1, 2, 2 | a Boolean |
| 19 to 24 | `EQ`, `NE`, `LT`, `LE`, `GT`, `GE` | none | 2 | a Boolean, false when either operand is absent |
| 25 | `IN` | none | 2: a value, then a collection | a Boolean |
| 26 | `BETWEEN` | none | 3: a value, a lower and an upper bound | a Boolean |
| 27, 28 | `IS_SOME`, `IS_NONE` | none | 1 | a Boolean |
| 29 | `IS_TYPE` | `arg`: a type's qualified name | 1, an instance or a catalog-type value | whether its type is at or below the named one |
| 30 | `IS_VALUE` | none | 2: an instance, then a catalog-type value | whether the instance's type is at or below the value |
| 32 to 35 | `ADD`, `SUB`, `MUL`, `DIV` | none | 2 | the result, absent when either operand is |
| 36 | `NEG` | none | 1 | the negation |
| 40 | `ALLEN` | `x`: the relation, 1 to 16 in the order `SCHEMA.md` section 4 lists `allen`; `arg`: 0 as written, 1 `certainly`, 2 `possibly` | 2 | a Boolean |
| 44 | `LIST` | `arg`: the element count | `arg` | the list |
| 48, 49, 50 | `SOME`, `NO`, `ALL` | `arg`: the filter program's index, lower than its own, or `ABSENT` for no filter | 1, a collection | a Boolean; `ALL` true over no element |
| 51 | `EXISTS` | none | 1, a collection or an optional value | whether it holds anything |
| 52 to 55 | `COUNT`, `SUM`, `MIN`, `MAX` | none | 1, a collection | the aggregate |

**Verification.** A decoder accepts a program only when: every `op` is listed and its unused fields are zero;
every `arg` naming a string, a constant or a program is in bounds, and a filter's index is lower than the index of
the program naming it and names a program whose `filter` is 1; `ELEMENT` stands only in a program whose `filter` is
1; walking the instructions with each opcode's `Pops` and `Pushes`, the stack never holds fewer values than an
opcode pops, never more than `depth`, reaches `depth` at least once, and holds exactly one value at the end; and
the programs' instructions tile the instruction section, each program starting where the one before ends. A version
holding a program that fails any of these is refused whole.

**Evaluation** is `SCHEMA.md` section 4's: two-valued, an attribute's check vacuous over an absent value, failing
closed. An arithmetic overflow, a division by zero and an aggregate past its type's range end the evaluation with
the fault, which refuses the commit (`UGS1039`). Evaluation is a loop over the instructions with a stack of at
most `depth` values: it does not recurse, since a filter's index is lower than its caller's and so the programs of
an entry form no cycle, and it allocates nothing.

## 5. The entry kinds

Each kind gives its fixed part and its sections before the two string sections that end every entry. A field of
type "string" is a u32 string index; "or `ABSENT`" marks an optional one.

### 5.1 Schema (1) and Module (2)

Schema, fixed part 32 bytes:

| Offset | Width | Field | Type |
|---|---|---|---|
| 0 | 8 | `version` | u64: the schema's version, at least 1 |
| 8 | 4 | `name` | string |
| 12 | 4 | `base` | string: the vocabulary base |
| 16 | 4 | `prefix` | string: the prefix the `prefix` header names |
| 20 | 4 | `doc` | string or `ABSENT` |
| 24 | 1 | `open` | u8 boolean: `@open` |
| 25 | 7 | zero | 0 |

Module, fixed part 40 bytes: `version` u64 at 0; `extends_min` u64 at 8, the least version of what it extends;
`name`, `base`, `prefix`, `doc` at 16, 20, 24 and 28; `extends` at 32, a string, the extended schema's or module's
name; `open` at 36; bytes 37 to 39 zero.

| Section | Contents |
|---|---|
| 0 | imports, 24 bytes each: `kind` u8, 1 a vocabulary, 2 a schema; 7 zero bytes; `name` a string, the prefix or the schema's name; `target` a string, the vocabulary's IRI, `ABSENT` for a schema; `min_version` u64, a schema import's bound, 0 for none and for a vocabulary |
| 1 | reserved names, u32 strings |

### 5.2 Type (3)

Fixed part 64 bytes:

| Offset | Width | Field | Type |
|---|---|---|---|
| 0 | 2 | `prefix` | u16: the type's prefix, at least 3 |
| 2 | 1 | `kind` | u8: 1 statement, 2 entity, 3 relation |
| 3 | 1 | `flags` | u8: bit 0 abstract, bit 1 `@open`, bit 2 `@symmetric`, bit 3 `@multi`, bit 4 `@bitemporal`, own or inherited, bit 5 `@access`, bit 6 keyed, own or inherited; bit 7 zero |
| 4 | 1 | `mutability` | u8: 1 mutable, 2 bitemporal, 3 append-only |
| 5 | 1 | `encoding` | u8: 0 no relation, 1 edge, 2 node |
| 6 | 1 | `acyclic` | u8: 0 none, 1 `@acyclic`, 2 `@acyclic(open)` |
| 7 | 1 | zero | 0 |
| 8 | 4 | `name` | string: the type's name |
| 12 | 4 | `schema` | string: the declaring schema's or module's name |
| 16 | 4 | `parent` | string, a qualified name, or `ABSENT` |
| 20 | 4 | `key_space` | string: the qualified name of the type declaring the key, or `ABSENT` for a keyless type |
| 24 | 4 | `iri_template` | string or `ABSENT` |
| 28 | 4 | `anchor` | string: the anchor role's name; `ABSENT` for no relation and for a keyed one |
| 32 | 4 | `valid` | string: the valid interval's attribute, or `ABSENT` |
| 36 | 4 | `retracts` | string: the `@retracts` role, or `ABSENT` |
| 40 | 4 | `ends` | string: the `@ends` role, or `ABSENT` |
| 44 | 4 | `max_depth` | u32: 0 for none |
| 48 | 4 | `doc` | string or `ABSENT` |
| 52 | 2 | `key_space_prefix` | u16: the key space's prefix, 0 for a keyless type |
| 54 | 10 | zero | 0 |

| Section | Contents |
|---|---|
| 0 | key attributes, u32 strings, the attributes' names in the order `@key` lists them; empty for a keyless type |
| 1 | fields, u32 strings: the qualified names of the Attribute entries of the type's fields, own and inherited |
| 2 | roles, u32 strings: the qualified names of the Role entries of the type's roles, own and inherited; empty for no relation |
| 3 | actions, 8 bytes each: `role` a string; `on_delete` u8, 0 undeclared, 1 cascade, 2 restrict, 3 orphan; `on_retract` u8, 0 undeclared, 1 cascade, 2 restrict, 3 restrict open; 2 zero bytes |
| 4 | limits, 24 bytes each: `aggregate` u8, 1 `count`, 2 `sum`; `op` u8, 1 `<=`, 2 `>=`, 3 `=`; `per_type` u8 boolean; a zero byte; `attribute` a string, `sum`'s attribute, or `ABSENT`; `bound` a constant; `condition` a program, the conjuncts filtering the instances counted, or `ABSENT`; `select` a program, the conjuncts selecting the groups, or `ABSENT`; `per_first` u16 and `per_len` u16, its roles in section 5 |
| 5 | per roles, u32 strings: the role names the limits and the no-overlap declarations group by |
| 6 | no-overlap declarations, 8 bytes each: `strict` u8 boolean; 3 zero bytes; `per_first` u16 and `per_len` u16 |
| 7 | instance checks, 8 bytes each: `program` u32; `source` a string, the check's text, which a refusal's message quotes |
| 8 | `@maps_to` targets, u32 strings, each an IRI |
| 9 | the type table (section 3.1) |
| 10 | constants (section 3.2) |
| 11 | programs (section 4) |
| 12 | instructions (section 4) |

### 5.3 Prefix ranges (4)

Fixed part 8 bytes: `prefix` u16 at 0, the type's own; bytes 2 to 7 zero. Section 0 holds the type's prefix set as
ranges of 4 bytes, `first` u16 and `past` u16, half-open, ascending, no two adjacent or overlapping; the first
range holding `prefix`.

### 5.4 Attribute (5) and Named attribute type (11)

Attribute, fixed part 32 bytes:

| Offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `name` | string: the attribute's name |
| 4 | 4 | `type_name` | string: the declaring type's qualified name |
| 8 | 4 | `predicate` | string: the predicate's IRI |
| 12 | 4 | `type` | u32: its descriptor's index |
| 16 | 4 | `default` | u32: a constant, or `ABSENT` |
| 20 | 4 | `doc` | string or `ABSENT` |
| 24 | 1 | `flags` | u8: bit 0 optional, bit 1 immutable, a key attribute's set, bit 2 a key attribute, bit 3 `@unique`, bit 4 `@idempotency` (T48, C19); bits 5 to 7 zero |
| 25 | 1 | `indexes` | u8: bit 0 `range`, bit 1 `fulltext`, bit 2 `interval`, bit 3 `geo`, bit 4 `ann`; bits 5 to 7 zero |
| 26 | 1 | `metric` | u8: an `ann` index's, 1 `cosine`, 2 `dot`, 3 `l2`; 0 without one |
| 27 | 1 | `quantization` | u8: 0 none, 1 `int8` |
| 28 | 4 | zero | 0 |

Named attribute type, fixed part 24 bytes: `name`, `schema`, `type` and `doc` at 0, 4, 8 and 12; `flags`,
`indexes`, `metric` and `quantization` at 16 to 19, as an attribute's, bits 0 and 2 of `flags` zero; bytes 20 to 23
zero.

| Section | Contents |
|---|---|
| 0 | value checks (section 3.3) |
| 1 | values, u32 constants |
| 2 | `@maps_to` targets, u32 strings |
| 3 | the type table |
| 4 | constants |
| 5 | programs |
| 6 | instructions |

### 5.5 Predicate (6)

Fixed part 24 bytes:

| Offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `iri` | string |
| 4 | 4 | `datatype` | string: the datatype's IRI; `ABSENT` for an edge |
| 8 | 1 | `kind` | u8: 0 edge, 1 property |
| 9 | 1 | `cardinality` | u8: 1 single, 2 multi |
| 10 | 1 | `flags` | u8: bit 0 unique, bit 1 a count index, bit 2 a vector index; bits 3 to 7 zero |
| 11 | 1 | `facts` | u8, the request-typing facts of `SCHEMA.md` section 9.6: bit 0 a write of it is blind, bit 1 it needs a claim, bit 2 a rule reading beyond the instance reads it, bit 3 it is derived; bits 4 to 7 zero |
| 12 | 1 | `mutability` | u8: 1 mutable, 2 bitemporal, 3 append-only, when every declaring type agrees; 0 when they differ |
| 13 | 3 | zero | 0 |
| 16 | 2 | `dimension` | u16: a vector's; 0 otherwise |
| 18 | 1 | `metric` | u8: as an attribute's |
| 19 | 1 | `quantization` | u8: as an attribute's |
| 20 | 4 | zero | 0 |

| Section | Contents |
|---|---|
| 0 | tokenizers, u32 strings, the names the tokenizer registry gives them, ascending as UTF-8 bytes |
| 1 | domains, u32 strings: the qualified names of the types declaring it, ascending as UTF-8 bytes |

These are the fields `PROTOCOL.md` section 4.6's kind-8 record carries, with the facts beside them; Describe's
sections 5 and 6 are derived from the Predicate, Type and Role entries.

A version holding a `@bitemporal` type holds two Predicate entries no Attribute entry names,
`ugdb:asserted_start_epoch` and `ugdb:asserted_end_epoch`, which the compiler declares for `asserted` as stored
(`SCHEMA.md` sections 7.4 and 12.4; T48, C30): single-valued properties of datatype `xsd:unsignedLong`, their domains
the `@bitemporal` types, with no flag and no fact, since the engine alone writes them, and `mutability` as their
domains give it.

### 5.6 Role (7) and Navigation (8)

Role, fixed part 40 bytes:

| Offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `name` | string |
| 4 | 4 | `relation` | string: the declaring relation's qualified name |
| 8 | 4 | `super_role` | string: the qualified name of the role it specializes, or `ABSENT` |
| 12 | 4 | `navigation` | string: its navigation name, or `ABSENT` for `_` |
| 16 | 4 | `predicate` | string: the role's predicate IRI in a node-encoded relation, the navigation's in an edge-encoded relation's anchor role, else `ABSENT` |
| 20 | 4 | `doc` | string or `ABSENT` |
| 24 | 4 | `min` | u32: the navigation's lower bound; 0 for `_` |
| 28 | 4 | `max` | u32: its upper bound; `ABSENT` for none and for `_` |
| 32 | 1 | `flags` | u8: bit 0 `Optional`, bit 1 set-valued, bit 2 the anchor role; bits 3 to 7 zero |
| 33 | 1 | `catalog` | u8: 0 the participants are instances, 1 `schema.EntityType`, 2 `schema.RelationType` |
| 34 | 6 | zero | 0 |

Section 0 holds the participant types, u32 strings, qualified names, the union's members as written; section 1 the
`@maps_to` targets.

Navigation, fixed part 32 bytes: `name` at 0; `type` at 4, the qualified name of the type gaining it; `relation`
at 8; `role` at 12, the name of the role whose participant gains it; `min` and `max` at 16 and 20, as a role's;
`direction` u8 at 24, 1 from the anchor, 2 a reverse read; `yields` u8 at 25, 1 the participant at the other role, 2
the relation's instances; bytes 26 to 31 zero. Section 0 holds its `@maps_to` targets.

### 5.7 Enum (9), Record (10), Quantity (12), Unit (13), Graph (16)

| Kind | Fixed part | Sections |
|---|---|---|
| Enum | 16 bytes: `name`, `schema`, `doc` at 0, 4, 8; `open` u8 at 12; 3 zero bytes | 0: members, u32 strings, its own in declaration order, then each module's in the order of section 6 |
| Record | 16 bytes: `name`, `schema`, `doc`; 4 zero bytes | 0: fields, 16 bytes each: `name` a string; `type` a descriptor; `optional` u8; 3 zero bytes; `checks_first` u16 and `checks_len` u16, into section 1. 1: value checks. 2: values. 3: the type table. 4: constants |
| Quantity | 32 bytes: `name`, `schema`, `dimension`, `canonical` (a unit's qualified name or `ABSENT`), `doc`; `scale` u8 at 20, 1 ratio, 2 interval, 3 ordinal, 4 nominal; 3 zero bytes; `number` u32 at 24, the quantity kind's number, at least 1 (section 3.1); 4 zero bytes | 0: `@maps_to` targets |
| Unit | 40 bytes: `name`, `schema`, `ucum`, `symbol`, `multiplier`, `offset`, `doc`, strings at 0 to 24, the multiplier and the offset rationals in section 3.2's form and `ABSENT` under `affine: false`; `affine` u8 at 28; 3 zero bytes; `number` u32 at 32, the unit's number, at least 1 (section 3.1); 4 zero bytes | 0: quantities, u32 strings, qualified names. 1: `@maps_to` targets |
| Graph | 24 bytes: `name`; `schema`, the name of the schema it is a graph of; `iri`; `base`, its instance base; `doc`; `default` u8 at 20; 3 zero bytes | none |

A record's field takes no program: its checks are of kinds 1 to 3.

### 5.8 Derivation (14) and Function (15)

Derivation, fixed part 40 bytes:

| Offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `name` | string |
| 4 | 4 | `type` | string: the qualified name of the type it is declared on |
| 8 | 4 | `result` | u32: its result's descriptor |
| 12 | 4 | `program` | u32: a computed derivation's program; `ABSENT` for a path |
| 16 | 4 | `body` | string: the body's source text, from its first token to its last |
| 20 | 4 | `doc` | string or `ABSENT` |
| 24 | 4 | `min` | u32: the result's least count |
| 28 | 4 | `max` | u32: its greatest, or `ABSENT` |
| 32 | 1 | `flags` | u8: bit 0 materialized, bit 1 computed, bit 2 `@index(range)`, bit 3 `exists`; bits 4 to 7 zero |
| 33 | 7 | zero | 0 |

Sections 0 to 3 are the type table, constants, programs and instructions; section 4 holds a path's steps, 16 bytes
each, in the path's order, at least one and at most 32, and none for a computed derivation or for a path with a step
naming a function, which stage 7 compiles:

| Step offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `kind` | u8: 1 a navigation name, 2 a role, 3 a derived name |
| 1 | 1 | `repeat` | u8: 0 once, 1 `+`, 2 `{m, n}` |
| 2 | 2 | zero | 0 |
| 4 | 4 | `name` | string: the qualified name of its Navigation, Role or Derivation entry |
| 8 | 2 | `min` | u16: `m` for `{m, n}`; 0 otherwise |
| 10 | 2 | `max` | u16: `n` for `{m, n}`, at least `min`; 0 otherwise |
| 12 | 4 | `filter` | u32: the program of the step's filters, joined by `and`; `ABSENT` for none |

Function, fixed part 40 bytes: `name` at 0; `type` at 4, a method's type or `ABSENT` for a free function; `schema`
at 8; `result` at 12; `body` at 16; `doc` at 20; `min` and `max` at 24 and 28; `flags` u8 at 32, bit 0 monotonic;
bytes 33 to 39 zero. Section 0 holds its parameters, 12 bytes each, `name` a string, `type` a descriptor,
`optional` u8 and 3 zero bytes; section 1 the type table.

### 5.9 Classification (17) and Report (18)

Classification, fixed part 8 bytes: `type` at 0, a string, the type's qualified name; bytes 4 to 7 zero. Section
0 holds the type's write operations, 16 bytes each, by `operation`, then by `member` as UTF-8 bytes:

| Operation offset | Width | Field | Type |
|---|---|---|---|
| 0 | 1 | `operation` | u8: 1 create or insert, 2 update an attribute, 3 change a role, 4 delete, 5 retract |
| 1 | 1 | `path` | u8: 1 coordination-free, 2 segmented, 3 coordinated |
| 2 | 1 | `local` | u8 boolean: the state its deciding constraint reads is the transaction's partition's by placement |
| 3 | 1 | zero | 0 |
| 4 | 4 | `member` | string: the attribute's or the role's name for operations 2 and 3; else `ABSENT` |
| 8 | 4 | `by` | string: the deciding constraint, as its annotation is written, `@key` or `@limit`; `ABSENT` for none |
| 12 | 4 | `indexes` | u32: the index entries the operation writes |

These are `SCHEMA.md` section 9.5's `UGS0901` records. Only the operations the version permits are listed, by these
rules, from the declarations alone:

- **Operations.** An entity type lists create, an update per attribute, and delete; a relation type insert, an update
  per attribute, a role change per role that is `Optional` or set-valued, and delete. A key attribute and a derived
  name take no update. A `@bitemporal` type lists retract in place of delete, its updates being revisions; an
  `@append_only` type lists its create or insert alone. An abstract type lists nothing.
- **`path` and `by`.** The operation's path is the most demanding of the rows of `SCHEMA.md` section 9.3 its
  constraints fall under, coordinated over segmented over coordination-free and, at one path, state another
  partition owns over the transaction's own; `by` names the constraint of that row, the first in this order on a
  tie: `@key`,
  `@unique`, `cardinality`, `@set`, `@acyclic`, `@limit`, `@no_overlap`, `restrict`, `restrict-open`, `cascade`,
  `@retracts` or `@ends`, written without arguments. A coordination-free operation has `by` `ABSENT`, rendered `none`,
  and `local` 1.
- **`local`.** 1 when the row's state is the transaction's partition's by placement, as `SCHEMA.md` section 9.3's "local" says; 0
  for every coordinated operation and a segmented one whose owner is elsewhere, as `@unique`'s claim is.
- **`indexes`.** The index entries one instance's operation puts or deletes, counted per declared index whatever the
  instance's values: an attribute's `@index(range)`, `@index(fulltext)` and `@index(geo)` one each, its
  `@index(interval)` two, an `@index(ann)` none while it is unbuilt, and a role whose navigation name has a bounded
  cardinality one, its count index. A `@key` or `@unique` attribute's implicit index is its claim
  (`../storage/STORAGE.md` section 9.3, record kind 4 or 5), and writes no index entry; a materialized derivation's index entries are not counted. A create,
  insert or delete counts every index of the type's attributes and roles; an update of an attribute twice that
  attribute's, the old entries deleted and the new put; a role change twice that role's count index; a retraction its roles' count
  indexes, which count live instances.

Report has no fixed part. Section 0 holds the other records of section 9.5 there, 16 bytes each, by `code`, then by
`subject` as UTF-8 bytes:

| Record offset | Width | Field | Type |
|---|---|---|---|
| 0 | 2 | `code` | u16: 902 to 906 |
| 2 | 2 | zero | 0 |
| 4 | 4 | `subject` | string: the record's subject, a qualified name |
| 8 | 4 | `a` | 902: the construct, a string: the annotation as written without arguments, `@index(ann)`, or the keyword, `fn` or `derive`; 903: the role to colocate, a string; 904: the constraint kept local, a string in `by`'s words; 905: the relation's qualified name, a string; 906: the index entries, a u32 |
| 12 | 4 | `b` | 902: the stage, a string, as `SCHEMA.md` section 14 numbers it, `7`, `8` or `9.5`; 904: the constraint routed through coordination, a string in `by`'s words; 905: the role, a string; 906: the rank, 1 to 10; 0 for 903 |

A `UGS0906` record is written for each of the ten operations writing most index entries, those writing none left
out, ties ranked in the Classification entries' order. A `UGS0905` record of a derivation names the relation and role
of the derivation's first step.

### 5.10 Source (19)

The source's text, UTF-8, byte for byte as it was installed. It is no body.

### 5.11 Layout (21)

The compiled layout of the tenant's data: which families of tiles the store keeps, and how (`../storage/STORAGE.md`
sections 13.4 and 14.3, T35). The schema compiler writes it once per version holding a Schema entry, and every replica
decodes it and never recompiles it, so a release cannot change a replica's tiles; a version holding no Schema entry has
none, and its tenant takes the generic layout. A family here is what a tile's family key holds apart from the tenant,
the graph and the bucket group, which the store adds per tile. The entry lists adjacency and column chunk families
alone: record runs, identity columns and slot maps exist in every tenant whatever it declares (`../storage/STORAGE.md`
section 13.4), so no Layout entry names them. Describe carries no Layout entry: it describes the
store's bytes, not the schema.

No fixed part.

| Section | Contents |
|---|---|
| 0 | families, 24 bytes each |
| 1 | inline columns, 12 bytes each |
| 2 | types, 12 bytes each: each type whose instances hold column chunks, by its qualified name as UTF-8 bytes |
| 3 | the type table (section 3.1) |

| Family offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `predicate` | string: the predicate's IRI |
| 4 | 4 | `type` | string: a column chunk's type, a qualified name; `ABSENT` for an adjacency family |
| 8 | 1 | `shape` | u8: 1 an adjacency block, 2 a column chunk |
| 9 | 1 | `kind` | u8: the shape's kind, 1 to 5 for an adjacency block (`../storage/STORAGE.md` section 7.1), 1 fixed width or 2 variable width for a column chunk (section 8.1 there) |
| 10 | 1 | `direction` | u8: an adjacency family's, as its kind gives it (`../storage/STORAGE.md` section 7.1): 1 forward for kinds 1, 2 and 5, 2 reverse for kind 3, 0 for an index relation, kind 4; 0 for a column chunk |
| 11 | 1 | `class` | u8: 1 storage class I, 0 class M |
| 12 | 1 | `tokenizer` | u8: an index relation's (kind 4) tokenizer identifier byte, not `0xFF`; 0 otherwise |
| 13 | 1 | `order` | u8: an adjacency family's row order, as a tile header's (section 6.2 there): 0 by neighbour, `1 + i` ascending and `0x81 + i` descending by inline column `i`; 0 for a column chunk |
| 14 | 1 | `inline` | u8: an adjacency family's inline columns, 0 to 8; 0 for a column chunk |
| 15 | 1 | zero | 0 |
| 16 | 4 | `inline_first` | u32: the index of its first inline column in section 1, where the family before's end |
| 20 | 4 | `descriptor` | u32: the descriptor of the values the family holds by sortable encoding: a column chunk's, a kind-2 family's elements', a kind-5 family's values'; `ABSENT` for any other |

| Inline column offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `attribute` | string: the predicate IRI of the attribute whose value the column holds |
| 4 | 4 | `descriptor` | u32: its descriptor's index |
| 8 | 1 | `width` | u8: the descriptor's sortable width, 1 to 18 (`../storage/STORAGE.md` section 5.2; T48, C25) |
| 9 | 3 | zero | 0 |

| Type offset | Width | Field | Type |
|---|---|---|---|
| 0 | 4 | `type` | string: the type's qualified name |
| 4 | 4 | `first` | u32: the index of its first column-chunk family in section 0 |
| 8 | 4 | `count` | u32: its column-chunk families, at least 1, contiguous from `first` |

Families come by `shape`, then by `type` as UTF-8 bytes, `ABSENT` first, then by `predicate` as UTF-8 bytes, then by
`kind`, `direction`, `class` and `tokenizer`; no two agree in all of these, so a type's column chunks are contiguous. A
predicate a type of each storage class declares is two families (T36). Inline columns are numbered from 0 within their
family, in section 1's order, which is the order its `order` field counts them by; a column that orders the family
names a required attribute (`SCHEMA.md` section 5.2).

A version holding a `@bitemporal` type lists, for each `@bitemporal` type, a fixed-width column
chunk family of `ugdb:asserted_start_epoch` in storage class I and one of `ugdb:asserted_end_epoch` in class M, each of
descriptor `UInt64`, among the type's column chunks and in their order, as any attribute's (`SCHEMA.md` section 12.4;
T48, C30). The four bound attributes of an `Interval` attribute are listed as attributes too: the bounds of `T`'s
descriptor, the precisions of tag 37's (`SCHEMA.md` section 12.2).

## 6. One encoding: what a decoder checks

A decoder refuses a version at the first rule broken, naming the rule and the offset, in this order:

| Step | Rule |
|---|---|
| 1 | The bytes hold the fixed part; `magic` and `format` are this file's |
| 2 | `checksum` is XXH3-64, seed 0, of the bytes from offset 8 |
| 3 | The fixed part's zero bytes are zero; `version` is at least 1; `prefix_next` is at least 3; `predicate_next` is at least 2^63; `unit_last` and `quantity_last` are at least every number a Unit or a Quantity entry holds |
| 4 | The body layout holds for seven sections (`PROTOCOL.md` section 4.1) |
| 5 | The names are a string list (section 4.3 there), each distinct once |
| 6 | The entry table is a whole number of records and holds `entries` of them; each record's zero bytes are zero, its `kind` 1 to 19 or 21, its `name` in bounds, its `offset` where the entry before ends, its bytes within section 1, and it comes after the entry before by kind, then by name; section 1 ends with the last entry; the names are interned in the table's order |
| 7 | The kind directory holds, for each kind, the count of entries of that kind and below |
| 8 | Each entry, in order: for kinds 1 to 18 and 21, its body layout with its kind's fixed part and section count; its fixed part's rules and zero bytes; each section a whole number of its records; every string, descriptor, constant and program index in bounds; its strings interned in order of first use, none unused; its descriptors, constants and programs as sections 3 and 4 state. For kind 19, its bytes UTF-8. For kind 21, its families, inline columns and types as section 5.11 states: each field's value within its row's range, the families in their order and each family's inline columns where the family before's end, section 1 ending with the last family's, `order` naming one of the family's inline columns or 0, each inline column's `width` its descriptor's sortable width, and each type's families its column chunks and no other |
| 9 | No two Unit entries hold one number, and no two Quantity entries. The prefix index holds one record per Type entry, `types` of them, ascending, each naming a Type entry whose `prefix` is the record's; no two types share a prefix; every prefix is below `prefix_next`. The predicate index holds one record per Predicate entry, its zero bytes zero, strictly ascending by `sequence`, every sequence at least 2^63 and below `predicate_next`, each naming a Predicate entry, and every Predicate entry named once |
| 10 | Every name an entry holds resolves: a qualified name to the entry of that name and kind, an attribute's predicate to its Predicate entry, a Type entry's fields and roles to Attribute and Role entries, of which at most one field's Attribute entry sets `flags` bit 4, `@idempotency`, inherited fields counted (T48, C19), each type to one Prefix ranges and one Classification entry; one Report entry exists; one Layout entry exists when a Schema entry does, and none otherwise, every predicate it names resolving to a Predicate entry and every type to a Type entry |
| 11 | Each entry's name in the table is the one its fields give: a Type's `{schema}.{name}`, an Attribute's `{type_name}.{name}`, a Predicate's `iri`, and so on by the table of section 2 |

Encoding a decoded version gives the bytes decoded. What the encoding leaves to the encoder is nothing: lists
keep source order, strings and constants are interned in order of first use, and every other order is stated above.

Install holds the compiler to this file before anything is committed: it decodes the version it compiled by these
steps, with the decoder every role uses, and encodes the decoded version back to the same bytes. A version failing
either is the compiler's fault, and the Install is refused with nothing allocated or committed (ugs-catalog), so no
role meets a version it cannot read because of the compiler.

## 7. What a role holds decoded

A role that loads a version verifies it once (section 6) and then holds it as views over the bytes: the entry
table, the prefix index, the predicate index, and per entry its fixed part and sections, read in place. The bytes are
one immutable buffer per process, shared by every thread that reads the version and copied by none; a reply carrying
the version or its entries writes slices of it. It builds, once, on the control plane:

- from the prefix index, the Type entry of each prefix, and from each Type entry's fields and roles the indices of
  their Attribute and Role entries;
- from the predicate index and the tenant, each declared predicate's system ID, and for each Predicate entry its
  sequence, so a predicate IRI resolves by a search over the Predicate entries, which are ordered by IRI;
- the parsed value of each constant, and each `@pattern` compiled, with a matching cache for each thread that runs
  it.

After that a commit reaches a type from an instance's prefix, a predicate's declaration from its ID, the type's
fields, roles, checks and prefix ranges by index, and runs a check as a loop (section 4). It compares no name, parses
nothing, takes no lock and allocates nothing in the catalog, and it resolves no declared predicate through the
dictionary. A role that cannot verify a version its log or its metadata group names stops.

## 8. Worked example

The bytes below are the version as `ultragraphdb_core_catalog` writes it before the Layout entry (section 5.11) and
the kind directory's 21 values; the change that implements them regenerates the bytes, with the version's Layout entry,
and this section with them: this file leads the code. Its `unit_last` and `quantity_last` are 0, bytes 56 to 63.

The source `demo`, 100 bytes, each line ending in one `0a`:

```
schema demo version 1
prefix d = "https://example.org/demo#"
entity Thing @key(id) {
  id: String
}
```

installed into tenant 0 as catalog version 1. `Thing` takes prefix 3, so `prefix_next` is 4, and the predicate
`https://example.org/demo#id` takes the tenant's first sequence, 2^63, so `predicate_next` is 2^63 + 1. The version is
1,384 bytes and holds eight entries:

| Index | Kind | Name | `offset` | `len` |
|---|---|---|---|---|
| 0 | 1 Schema | `demo` | 0 | 110 |
| 1 | 3 Type | `demo.Thing` | 110 | 258 |
| 2 | 4 Prefix ranges | `demo.Thing` | 368 | 40 |
| 3 | 5 Attribute | `demo.Thing.id` | 408 | 175 |
| 4 | 6 Predicate | `https://example.org/demo#id` | 583 | 156 |
| 5 | 17 Classification | `demo.Thing` | 739 | 86 |
| 6 | 18 Report | the empty string | 825 | 24 |
| 7 | 19 Source | `demo` | 849 | 100 |

**The version's fixed part and section table**, bytes 0 to 119:

| Bytes | Hex | Field | Value |
|---|---|---|---|
| 0–7 | `12 3c 61 5b d0 ef 7e 40` | `checksum` | `0x407EEFD05B613C12`, XXH3-64 of bytes 8 to 1,383 |
| 8–11 | `55 47 43 56` | `magic` | `UGCV` |
| 12–13 | `01 00` | `format` | 1 |
| 16–23 | `00` × 8 | `tenant` | 0 |
| 24–31 | `01` then `00` × 7 | `version` | 1 |
| 32–35 | `08 00 00 00` | `entries` | 8 |
| 36–39 | `01 00 00 00` | `types` | 1 |
| 40–41 | `04 00` | `prefix_next` | 4 |
| 48–55 | `01` then `00` × 6 then `80` | `predicate_next` | 2^63 + 1 |
| 64–71 | `78 00 00 00 80 00 00 00` | section 0 | the entry table at 120, 128 bytes |
| 72–79 | `f8 00 00 00 b5 03 00 00` | section 1 | the entries' bytes at 248, 949 bytes |
| 80–87 | `b0 04 00 00 14 00 00 00` | section 2 | the names' ends at 1,200, 20 bytes: five names |
| 88–95 | `c8 04 00 00 36 00 00 00` | section 3 | the names' text at 1,224, 54 bytes |
| 96–103 | `00 05 00 00 08 00 00 00` | section 4 | the prefix index at 1,280: `03 00 00 00 01 00 00 00`, prefix 3 at entry 1 |
| 104–111 | `08 05 00 00 4c 00 00 00` | section 5 | the kind directory at 1,288, 76 bytes: 1, 1, 2, 3, 4, 5 for kinds 1 to 6, 5 for kinds 7 to 16, 6, 7, 8 |
| 112–119 | `58 05 00 00 10 00 00 00` | section 6 | the predicate index at 1,368, 16 bytes: `00` × 7 then `80`, then `04 00 00 00 00 00 00 00`, sequence 2^63 at entry 4 |

The names, interned in the entry table's order, are `demo`, `demo.Thing`, `demo.Thing.id`,
`https://example.org/demo#id` and the empty string; the Prefix ranges, the Classification and the Source entries
name strings already listed.

**The Attribute entry**, 175 bytes at byte 656 of the version (248 + 408), its offsets counted from its own
first byte:

| Bytes | Hex | Field | Value |
|---|---|---|---|
| 0–3 | `00 00 00 00` | `name` | string 0, `id` |
| 4–7 | `01 00 00 00` | `type_name` | string 1, `demo.Thing` |
| 8–11 | `02 00 00 00` | `predicate` | string 2, `https://example.org/demo#id` |
| 12–15 | `00 00 00 00` | `type` | descriptor 0 |
| 16–19 | `ff ff ff ff` | `default` | `ABSENT` |
| 20–23 | `ff ff ff ff` | `doc` | `ABSENT` |
| 24 | `06` | `flags` | immutable, a key attribute |
| 25–27 | `00 00 00` | `indexes`, `metric`, `quantization` | none |
| 32–103 | nine `offset`, `len` pairs | the section table | sections 0 to 2 empty at 104; section 3 at 104, 16 bytes; sections 4 to 6 empty at 120; section 7 at 120, 12 bytes; section 8 at 136, 39 bytes |
| 104–119 | `0e` then `00` × 15 | the type table | one descriptor, tag 14, `String` |
| 120–131 | `02 00 00 00 0c 00 00 00 27 00 00 00` | the strings' ends | 2, 12, 39 |
| 132–135 | `00` × 4 | padding | |
| 136–174 | `69 64`, `64 65 6d 6f 2e 54 68 69 6e 67`, then 27 bytes | the strings' text | `id`, `demo.Thing`, `https://example.org/demo#id` |

The other entries, in brief: the Schema entry holds version 1, the base and the prefix `d`, no import and no
reserved name; the Type entry prefix 3, kind 2, `flags` `40` (keyed), mutability 1, the key space `demo.Thing` with
its prefix 3, the key attribute `id` and the field `demo.Thing.id`; the Prefix ranges entry the one range 3 to 4;
the Predicate entry a single-valued property of datatype `http://www.w3.org/2001/XMLSchema#string`, unique, with
`facts` `02`, a claim, and the domain `demo.Thing`; the Classification entry two operations, a create, segmented
and local by `@key`, and a delete, coordination-free; the Report entry no record.

**The whole version**, 1,384 bytes:

```
0000  12 3c 61 5b d0 ef 7e 40 55 47 43 56 01 00 00 00
0010  00 00 00 00 00 00 00 00 01 00 00 00 00 00 00 00
0020  08 00 00 00 01 00 00 00 04 00 00 00 00 00 00 00
0030  01 00 00 00 00 00 00 80 00 00 00 00 00 00 00 00
0040  78 00 00 00 80 00 00 00 f8 00 00 00 b5 03 00 00
0050  b0 04 00 00 14 00 00 00 c8 04 00 00 36 00 00 00
0060  00 05 00 00 08 00 00 00 08 05 00 00 4c 00 00 00
0070  58 05 00 00 10 00 00 00 01 00 00 00 00 00 00 00
0080  00 00 00 00 6e 00 00 00 03 00 00 00 01 00 00 00
0090  6e 00 00 00 02 01 00 00 04 00 00 00 01 00 00 00
00a0  70 01 00 00 28 00 00 00 05 00 00 00 02 00 00 00
00b0  98 01 00 00 af 00 00 00 06 00 00 00 03 00 00 00
00c0  47 02 00 00 9c 00 00 00 11 00 00 00 01 00 00 00
00d0  e3 02 00 00 56 00 00 00 12 00 00 00 04 00 00 00
00e0  39 03 00 00 18 00 00 00 13 00 00 00 00 00 00 00
00f0  51 03 00 00 64 00 00 00 01 00 00 00 00 00 00 00
0100  00 00 00 00 01 00 00 00 02 00 00 00 ff ff ff ff
0110  00 00 00 00 00 00 00 00 40 00 00 00 00 00 00 00
0120  40 00 00 00 00 00 00 00 40 00 00 00 0c 00 00 00
0130  50 00 00 00 1e 00 00 00 04 00 00 00 1d 00 00 00
0140  1e 00 00 00 00 00 00 00 64 65 6d 6f 68 74 74 70
0150  73 3a 2f 2f 65 78 61 6d 70 6c 65 2e 6f 72 67 2f
0160  64 65 6d 6f 23 64 03 00 02 40 01 00 00 00 00 00
0170  00 00 01 00 00 00 ff ff ff ff 02 00 00 00 ff ff
0180  ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff ff
0190  ff ff 00 00 00 00 ff ff ff ff 03 00 00 00 00 00
01a0  00 00 00 00 00 00 b8 00 00 00 04 00 00 00 c0 00
01b0  00 00 04 00 00 00 c8 00 00 00 00 00 00 00 c8 00
01c0  00 00 00 00 00 00 c8 00 00 00 00 00 00 00 c8 00
01d0  00 00 00 00 00 00 c8 00 00 00 00 00 00 00 c8 00
01e0  00 00 00 00 00 00 c8 00 00 00 00 00 00 00 c8 00
01f0  00 00 00 00 00 00 c8 00 00 00 00 00 00 00 c8 00
0200  00 00 00 00 00 00 c8 00 00 00 00 00 00 00 c8 00
0210  00 00 14 00 00 00 e0 00 00 00 22 00 00 00 03 00
0220  00 00 00 00 00 00 04 00 00 00 00 00 00 00 05 00
0230  00 00 09 00 00 00 13 00 00 00 15 00 00 00 22 00
0240  00 00 00 00 00 00 54 68 69 6e 67 64 65 6d 6f 64
0250  65 6d 6f 2e 54 68 69 6e 67 69 64 64 65 6d 6f 2e
0260  54 68 69 6e 67 2e 69 64 03 00 00 00 00 00 00 00
0270  20 00 00 00 04 00 00 00 28 00 00 00 00 00 00 00
0280  28 00 00 00 00 00 00 00 03 00 04 00 00 00 00 00
0290  00 00 00 00 01 00 00 00 02 00 00 00 00 00 00 00
02a0  ff ff ff ff ff ff ff ff 06 00 00 00 00 00 00 00
02b0  68 00 00 00 00 00 00 00 68 00 00 00 00 00 00 00
02c0  68 00 00 00 00 00 00 00 68 00 00 00 10 00 00 00
02d0  78 00 00 00 00 00 00 00 78 00 00 00 00 00 00 00
02e0  78 00 00 00 00 00 00 00 78 00 00 00 0c 00 00 00
02f0  88 00 00 00 27 00 00 00 0e 00 00 00 00 00 00 00
0300  00 00 00 00 00 00 00 00 02 00 00 00 0c 00 00 00
0310  27 00 00 00 00 00 00 00 69 64 64 65 6d 6f 2e 54
0320  68 69 6e 67 68 74 74 70 73 3a 2f 2f 65 78 61 6d
0330  70 6c 65 2e 6f 72 67 2f 64 65 6d 6f 23 69 64 00
0340  00 00 00 01 00 00 00 01 01 01 02 01 00 00 00 00
0350  00 00 00 00 00 00 00 38 00 00 00 00 00 00 00 38
0360  00 00 00 04 00 00 00 40 00 00 00 0c 00 00 00 50
0370  00 00 00 4c 00 00 00 02 00 00 00 00 00 00 00 1b
0380  00 00 00 42 00 00 00 4c 00 00 00 00 00 00 00 68
0390  74 74 70 73 3a 2f 2f 65 78 61 6d 70 6c 65 2e 6f
03a0  72 67 2f 64 65 6d 6f 23 69 64 68 74 74 70 3a 2f
03b0  2f 77 77 77 2e 77 33 2e 6f 72 67 2f 32 30 30 31
03c0  2f 58 4d 4c 53 63 68 65 6d 61 23 73 74 72 69 6e
03d0  67 64 65 6d 6f 2e 54 68 69 6e 67 00 00 00 00 00
03e0  00 00 00 20 00 00 00 20 00 00 00 40 00 00 00 08
03f0  00 00 00 48 00 00 00 0e 00 00 00 01 02 01 00 ff
0400  ff ff ff 01 00 00 00 00 00 00 00 04 01 01 00 ff
0410  ff ff ff ff ff ff ff 00 00 00 00 0a 00 00 00 0e
0420  00 00 00 64 65 6d 6f 2e 54 68 69 6e 67 40 6b 65
0430  79 18 00 00 00 00 00 00 00 18 00 00 00 00 00 00
0440  00 18 00 00 00 00 00 00 00 73 63 68 65 6d 61 20
0450  64 65 6d 6f 20 76 65 72 73 69 6f 6e 20 31 0a 70
0460  72 65 66 69 78 20 64 20 3d 20 22 68 74 74 70 73
0470  3a 2f 2f 65 78 61 6d 70 6c 65 2e 6f 72 67 2f 64
0480  65 6d 6f 23 22 0a 65 6e 74 69 74 79 20 54 68 69
0490  6e 67 20 40 6b 65 79 28 69 64 29 20 7b 0a 20 20
04a0  69 64 3a 20 53 74 72 69 6e 67 0a 7d 0a 00 00 00
04b0  04 00 00 00 0e 00 00 00 1b 00 00 00 36 00 00 00
04c0  36 00 00 00 00 00 00 00 64 65 6d 6f 64 65 6d 6f
04d0  2e 54 68 69 6e 67 64 65 6d 6f 2e 54 68 69 6e 67
04e0  2e 69 64 68 74 74 70 73 3a 2f 2f 65 78 61 6d 70
04f0  6c 65 2e 6f 72 67 2f 64 65 6d 6f 23 69 64 00 00
0500  03 00 00 00 01 00 00 00 01 00 00 00 01 00 00 00
0510  02 00 00 00 03 00 00 00 04 00 00 00 05 00 00 00
0520  05 00 00 00 05 00 00 00 05 00 00 00 05 00 00 00
0530  05 00 00 00 05 00 00 00 05 00 00 00 05 00 00 00
0540  05 00 00 00 05 00 00 00 06 00 00 00 07 00 00 00
0550  08 00 00 00 00 00 00 00 00 00 00 00 00 00 00 80
0560  04 00 00 00 00 00 00 00
```

An implementation's first test decodes these bytes to the entries above and encodes them back to the same bytes;
its second changes one byte and is refused at step 2.

## 9. Formats

`format` is 1. A decoder refuses a format it does not know, naming it, and a role that meets one stops. A later
format is a new number, never a reading of spare bytes:

- **Stage 7** gives a function's body a compiled form beside its text, and a path with a step naming a function its steps.
- **Stage 9.5** adds the retired-name entry, kind 20, whose 24 bytes `PROTOCOL.md` section 8.3 already lays out
  for Describe, and the migration entry of `SCHEMA.md` section 10.2.

A catalog version is written once and never rewritten, so a role reads every format its tenants' versions were
written in.

## 10. Decisions

| Question | Decision |
|---|---|
| The container | A body in `PROTOCOL.md` section 4.1's layout, the entry table Describe's record, so the wire's reading code and rules serve the catalog and Describe copies entries as they stand (user, 2026-11-10) |
| Self-contained entries | Each entry carries its own strings, types, constants and programs. It costs repeated names across entries, and it is what lets Describe carry a subset to a principal and the schema id hash an entry as stored |
| References by name | An entry names another by its qualified name, never by an index into the table, which a filtered Describe would not preserve; a role resolves names once, at load |
| Integrity | A magic, a format number and an XXH3-64 over everything after the checksum, checked before anything is read; a version that fails is refused whole and a role holding it stops |
| The prefix index | In the version, since every commit starts from an instance's prefix |
| The predicate index | In the version beside its entries, never in one: the metadata group allocates a declared predicate's ID with the version, so every role holds the same ID table complete at load and no commit or read resolves a declared predicate through the dictionary; and a system ID never leaves the database, while Describe carries entries as they stand (user, 2026-11-10, `stage-6-ugs-data-definition/notes/core-principles-review.md`, P7) |
| Expressions | Instance checks, limit conditions and computed derivations as verified postfix programs with a fixed 8-byte instruction, evaluated in a loop; the storage role needs no parser. A path derivation is its steps, which a partition walks to maintain a materialized one (user, 2026-11-10); function bodies stay text until the query core defines its compiled form at stage 7, since a plan's form is that stage's design |
| Constants | Canonical lexical forms with a type, parsed once at load: one encoding already specified for every datatype in `PROTOCOL.md` section 10.5, in place of a second, binary one |
| The request-typing facts | Fields of the Predicate entry, one fact in one place; the Classification entry holds a type's operations |
| The worked example | One schema of one entity with one attribute, every byte given, as `PROTOCOL.md` section 2.4 works `hello.frame`; larger versions are checked field by field through the decoder (`SCHEMA.md` section 10.2, rule 6) |
