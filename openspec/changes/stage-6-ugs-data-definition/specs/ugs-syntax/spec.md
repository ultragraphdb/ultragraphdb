## ADDED Requirements

### Requirement: A source is bounded UTF-8 text
A UGS source SHALL be UTF-8 text of at most 16 MiB with lines of at most 64 KiB, line breaks `\n` or `\r\n`, and
nesting of braces, brackets and parentheses together of at most 64 levels. An identifier SHALL be at most 128 bytes,
and a string literal, an `@iri` template and a `@pattern` at most 4 KiB. A source past any of these bounds SHALL be
refused with `UGS0013`, naming the bound, as `SCHEMA.md` section 3.7 lists them. The parser SHALL read a source in one
pass, SHALL hold its recursion within the nesting bound, and SHALL return a diagnostic, never panic, on any byte
sequence.

#### Scenario: Nesting past 64 levels is refused
- **WHEN** a source holds an expression nested in 65 pairs of parentheses
- **THEN** it is refused with `UGS0013` at the 65th opening parenthesis, naming the nesting bound of 64

#### Scenario: A line past 64 KiB is refused
- **WHEN** a source holds a line of 65,537 bytes
- **THEN** it is refused with `UGS0013` naming the line and the bound

#### Scenario: Bytes that are not UTF-8 are refused without a panic
- **WHEN** the bytes `FF FE 00` are given as a source
- **THEN** the parser returns a diagnostic naming byte offset 0 and no syntax tree

### Requirement: Comments and documentation comments
`//` SHALL run to the end of its line and `/*` … `*/` SHALL nest. A run of consecutive lines beginning with `///`
SHALL attach to the declaration, field, role, enum member, graph or header that follows it as its `doc`: the marker and
one following space removed per line, the lines joined by `\n`. A `///` run followed by anything else SHALL be dropped
with the warning `UGS0804`. An unterminated comment SHALL be refused with `UGS0002`.

#### Scenario: A documentation comment attaches to its declaration
- **WHEN** `/// Anything that can be owned,` and `/// assembled and supplied.` precede `abstract entity Asset`
- **THEN** the syntax tree's `Asset` has the `doc` `Anything that can be owned,\nassembled and supplied.`

#### Scenario: A nested block comment closes once per opening
- **WHEN** a source holds `/* a /* b */ c */ entity E`
- **THEN** the only declaration parsed is `E`

#### Scenario: A documentation comment attached to nothing warns
- **WHEN** a `///` run is followed by the end of the source
- **THEN** the source parses, and the diagnostics hold one warning `UGS0804` at the run

### Requirement: Identifiers, reserved words and contextual keywords
An identifier SHALL be an ASCII letter or `_` followed by ASCII letters, digits and `_`, case sensitive. The twenty
words of `SCHEMA.md` section 3.4's reserved list SHALL never be identifiers (`UGS0007`). `import`, `unit` and
`version` SHALL be keywords where a header or a declaration begins, and `version` after a schema's or module's name,
and identifiers everywhere else. Every other quoted word of the grammar SHALL be a keyword in the position the grammar
names and an identifier elsewhere. A name differing from a reserved word by case alone, and a name against the naming
convention of section 3.3, SHALL warn with `UGS0805`; unit names SHALL be left unchecked.

#### Scenario: A reserved word is refused as a name
- **WHEN** a source declares `entity relation { }`
- **THEN** it is refused with `UGS0007` at `relation`

#### Scenario: Operator words are names in declaration positions
- **WHEN** a relation declares the role `by: Person -> in*` and an enum the member `max`
- **THEN** the source parses, with a role named `by`, a navigation name `in` and an enum member `max`

#### Scenario: Header keywords are names elsewhere
- **WHEN** an entity declares the attributes `version: UInt32` and `unit: Unit`, and an enum the member `import`
- **THEN** the source parses, and the three are an attribute, an attribute and an enum member

#### Scenario: A word is an operator where the grammar expects one
- **WHEN** a `@check` holds `valid contains instant` and a function body holds `Node(id = node, valid at instant)`
- **THEN** the first compares the field `valid` with the Allen operator `contains`, and the second is the filter item
  `valid at`

### Requirement: Literals
The lexer SHALL read every literal of `SCHEMA.md` section 3.5: integers and decimals with `_` between digits, floats
with an exponent, rationals as two integers around `/`, strings with the escapes `\"`, `\\`, `\n`, `\t`, `\r` and
`\u{X}` of one to six hexadecimal digits naming a scalar value, Booleans, dates, date-times with an offset or none,
times, years and months, decades, centuries and millennia, ISO 8601 durations, intervals written with `..`, and IRIs
between `<` and `>`. It SHALL refuse a character outside the lexical structure with `UGS0001`, an unterminated string
or IRI with `UGS0002`, another escape with `UGS0003`, a malformed number, and the words `inf`, `-inf` and `nan` as
literals, with `UGS0004`, and a temporal literal naming no calendar instant or a duration with no part with `UGS0005`.
A four-digit `YYYY` SHALL lex as an integer and be read as a year by the expected type.

#### Scenario: A rational literal is exact
- **WHEN** a unit declares `multiplier: 1200 / 3937`
- **THEN** the syntax tree holds the rational with numerator 1200 and denominator 3937

#### Scenario: An interval literal with mixed precisions
- **WHEN** a default holds `2026-09-14 .. 2027`
- **THEN** the syntax tree holds an interval whose start is the day 2026-09-14 and whose end is the integer token
  2027, read as a year where a temporal bound is expected

#### Scenario: A date naming no day is refused
- **WHEN** a source holds the literal `2026-02-30`
- **THEN** it is refused with `UGS0005` at the literal

#### Scenario: An unknown escape is refused
- **WHEN** a source holds the string `"a\qb"`
- **THEN** it is refused with `UGS0003` at the escape

#### Scenario: A decade literal
- **WHEN** a source holds `1990s` where a temporal value is expected
- **THEN** the syntax tree holds the decade beginning in 1990

### Requirement: The grammar of section 4
The parser SHALL accept exactly the grammar of `SCHEMA.md` section 4: a source that is one schema or one module with
its headers and declarations, attribute, enum, record, statement, entity, relation, derive, function, quantity, unit
and graph declarations, fields with `Optional` and defaults, roles with participants, unions, `as` and a navigation
name with its cardinality or `_`, and types with their arguments. It SHALL produce a syntax tree in which every node
carries its source span. A token the grammar does not expect SHALL be refused with `UGS0006`, the message naming what
was expected; a role without a cardinality marker SHALL be refused with `UGS0403`. A type's annotations before and
after its body SHALL form one list in source order.

#### Scenario: The aviation example parses
- **WHEN** the source of `SCHEMA.md` section 16 is parsed
- **THEN** the syntax tree holds the schema `aviation` version 1, one `prefix`, one `import`, 3 named attribute types,
  1 enum, 7 entity types, 6 relation types, 2 derivations, 2 functions and 1 graph, in source order

#### Scenario: Gen5 parses
- **WHEN** `specification/schema/deepbrain-gen5.ugs` is parsed
- **THEN** the syntax tree holds the schema `deep_brain` version 5 with 76 entity, 87 relation and 2 statement
  declarations, 6 derivations, 14 functions and the graph `ontology`, and no diagnostic of kind error

#### Scenario: A role without a cardinality marker
- **WHEN** a relation declares `asset: Asset -> owner`
- **THEN** it is refused with `UGS0403` at `owner`, the fix naming the markers `?`, `*`, `+` and `{m, n}`

#### Scenario: Annotations before and after a body are one list
- **WHEN** `relation Supply : Agreement @append_only { … } @limit(…) @on_delete(…)` is parsed
- **THEN** `Supply`'s annotation list is `@append_only`, `@limit`, `@on_delete`, in that order

#### Scenario: A set-valued participant takes no Optional
- **WHEN** a role is written `members: [DerivesFrom] Optional -> alternative_set?`
- **THEN** it is refused with `UGS0006` at `Optional`

### Requirement: Annotations
An annotation SHALL be `@`, an identifier and an optional parenthesised argument list whose arguments are literals,
identifiers, qualified identifiers, CURIEs, IRI literals, lists, named arguments, expressions, a `@limit` argument or
role actions. An unknown annotation SHALL be refused with `UGS0008`, a malformed argument with `UGS0009`, an annotation
on an item its section does not allow with `UGS0011`, and an annotation repeated on one item with `UGS0010`, except
`@index` once per index kind, `@check` any number of times and `@maps_to` once per external term.

#### Scenario: An unknown annotation is refused
- **WHEN** a field carries `@indexed`
- **THEN** the source is refused with `UGS0008` at `@indexed`

#### Scenario: Two indexes of different kinds on one field
- **WHEN** a field declares `label: Text Optional @index(range) @index(fulltext)`
- **THEN** the source parses, and the field holds both indexes

#### Scenario: The same index kind twice is refused
- **WHEN** a field declares `@index(range) @index(range)`
- **THEN** the source is refused with `UGS0010` at the second annotation

#### Scenario: A limit argument parses
- **WHEN** a relation carries `@limit(count() <= 1 per (subject, type) where rank = preferred)`
- **THEN** the annotation holds the aggregate `count()`, the operator `<=`, the bound 1, the `per` items `subject` and
  `type`, and the condition `rank = preferred`

### Requirement: The expression grammar
The parser SHALL accept the expression grammar of `SCHEMA.md` section 4 for `@check`, `@limit`'s condition, derive
filters, computed derivations and function bodies, with the precedence the grammar gives: `or`, `and`, `not`,
comparison, sum, product, unary minus, primary. A derive body that is wholly a path, with or without `exists`, SHALL
be read as a path, and any other body as an expression. A step's filters SHALL precede its bound. An expression
outside the grammar SHALL be refused with `UGS0120`.

#### Scenario: A path derivation and a computed derivation
- **WHEN** `derive Asset.all_parts = parts+` and `derive Asserted.closed: Bool = not live or valid.end is some or rank
  = deprecated` are parsed
- **THEN** the first body is a path of one step with the transitive marker, and the second an `or` of three operands

#### Scenario: Filters precede the bound
- **WHEN** a function body holds `grounds(with retracted){1,32}`
- **THEN** the step `grounds` has the filter `with retracted` and the bound 1 to 32

#### Scenario: Precedence of not, and, or
- **WHEN** a `@check` holds `not a and b or c`
- **THEN** it parses as `((not a) and b) or c`

### Requirement: Diagnostics are rendered with location and fix
A diagnostic SHALL carry its code, its kind (error, warning or note), a source location of file, line, column and
length, a reason and, where one exists, a fix, and SHALL render in the form `SCHEMA.md` section 10.4 shows: the kind
and code with the reason, the location line, the source line with a marker under the span, and a `help:` block for the
fix. A source with a syntax error SHALL yield its diagnostics and no syntax tree for the compile.

#### Scenario: A diagnostic renders as the specification shows
- **WHEN** the file `aviation.ugs` holds an unknown annotation at line 12, column 20, of length 8
- **THEN** the rendering begins `error[UGS0008]:`, its second line is `  --> aviation.ugs:12:20`, and the marker line
  holds eight carets under the annotation

### Requirement: Every syntactic diagnostic has a corpus source
The conformance corpus SHALL hold, for each of `UGS0001` to `UGS0013` and `UGS0403`, a minimal source raising that
diagnostic and nothing else, with the expected code, location and fix, written independently of the parser. The parser
SHALL raise exactly the expected diagnostic for each.

#### Scenario: The corpus's syntax sources raise their diagnostics
- **WHEN** each source under the corpus's syntax folder is parsed
- **THEN** each yields exactly the one diagnostic its expectation names, at the expected line and column
