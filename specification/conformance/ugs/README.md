# The UGS conformance corpus

The corpus holds UGS sources and what a compiler must answer for each, written from `../../schema/SCHEMA.md` and
`../../schema/CATALOG.md` alone, never from a compiler's output. Stage 6's compiler is built against it, and a second
implementation, a client-side checker or a language server is held to the same files. `BUILD.bazel` exports every file
as the filegroup `//specification/conformance/ugs:corpus`.

## Layout

```
diagnostics/UGSnnnn/   one folder per diagnostic of SCHEMA.md section 10.4, and UGS2005 of phase 9
  expect.json          the cases
  *.ugs                the sources the cases name
valid/<name>/          sources that compile: aviation, the corpus's own modules, one per construct of section 14's stage 6 rows
  expect.json
  *.ugs
valid/gen5/expect.json Gen5's expectation; its source is ../../schema/deepbrain-gen5.ugs, the one copy
catalog/aviation.json  the aviation schema's catalog version, entry by entry and field by field
```

A source names its case in a leading comment, `// expect: UGS0201 at 3:9` for a refused source and `// expect: compiles`
for one that compiles and for a source several cases share, so a file reads on its own; a source that compiles with
warnings names them after it, ` with UGSnnnn at L:C` for the first and `, UGSnnnn at L:C` for each further one, in source
order: `// expect: compiles with UGS0903 at 10:14, UGS0904 at 12:3`. The comment and `expect.json` agree. A source copied from the specification, as aviation's is from `SCHEMA.md` section 16, stays
byte for byte as there, with no comment, so its locations are the specification's. Sources are named `reject.ugs` and
`accept.ugs`, or `reject_<slug>.ugs` and `accept_<slug>.ugs` where a code has several cases.

## A diagnostic's cases

`diagnostics/UGSnnnn/expect.json`:

```json
{
  "code": "UGS0201",
  "kind": "error",
  "cases": [
    {
      "name": "a field typed with an unknown type",
      "sources": ["reject.ugs"],
      "inputs": { "prefix_next": 3, "predicate_next": "9223372036854775808" },
      "diagnostics": [
        {
          "code": "UGS0201", "kind": "error",
          "file": "reject.ugs", "line": 4, "column": 9, "length": 5,
          "fix": [ { "file": "reject.ugs", "line": 4, "column": 9, "length": 5, "replacement": "String" } ]
        }
      ],
      "accept": { "sources": ["accept.ugs"], "warnings": [] }
    }
  ]
}
```

- `sources` are compiled together, in one Install into an empty tenant, in the order `SCHEMA.md` section 7.1 decides.
- `inputs` is optional: the compiler's next free prefix and next free predicate sequence (`SCHEMA.md` section 10.1),
  the latter a decimal string since it passes 2^53. Absent, they are 3 and 2^63.
- `diagnostics` is every diagnostic of kind error or warning the sources raise, in source order; the notes of the
  report (`UGS0901`, `UGS0905`, `UGS0906`) are not listed here. A refused source raises its code and nothing else.
- A location and a fix follow `SCHEMA.md` section 10.4: lines and columns from 1, a column and a length in Unicode
  scalar values. `fix` is the list of edits the diagnostic proposes, applied as given, or `null` where the diagnostic
  carries no concrete fix.
- `accept` is the twin: the sources with the fix applied, or, where `fix` is `null`, the smallest correction. They
  compile, and raise exactly the warnings listed, each as a diagnostic above.
- A warning's case has `"kind": "warning"`: its sources compile, with the warning in `diagnostics`; its twin applies
  the fix and raises no warning.
- A bound of `SCHEMA.md` section 3.7 whose source would pass 1 MiB is held by the syntax crate's unit tests, which
  build the source in memory, and appears here with its smaller bounds only.

## A valid source

`valid/<name>/expect.json`:

```json
{
  "name": "aviation",
  "sources": ["aviation.ugs"],
  "inputs": { "prefix_next": 3, "predicate_next": "9223372036854775808" },
  "warnings": [ { "code": "UGS0902", "kind": "warning", "file": "aviation.ugs", "line": 8, "column": 40, "length": 27, "fix": null } ],
  "prefixes": { "aviation.Asset": 3 },
  "report": [
    "UGS0901 aviation.Engine.create path=segmented local=true by=@key indexes=2"
  ]
}
```

- `sources` paths are relative to the folder; Gen5's is `../../../../schema/deepbrain-gen5.ugs`.
- `warnings` are the warnings in source order, each a diagnostic as above.
- `prefixes` maps every declared type's qualified name to the prefix phase 9 allocates it.
- `report` is the report's records rendered as `SCHEMA.md` section 9.5 renders them, one string per record, in the
  catalog version's canonical order: the Classification entries' `UGS0901` records by type name, within a type by
  operation (create or insert, update, role change, delete, retract) and then member; then the Report entry's records
  by code and then subject. Which operations a type lists, and their `path`, `local`, `by` and `indexes`, follow
  `CATALOG.md` section 5.9. A subject is `{type}.create`, `{type}.insert`, `{type}.update.{attribute}`,
  `{type}.role.{role}`, `{type}.delete` or `{type}.retract`, `{type}` the qualified name; a boolean renders `true` or
  `false`.

## The catalog version

`catalog/aviation.json` is the aviation schema's version as `CATALOG.md` lays it out, read through its decoder: the
fixed part's fields, then one object per entry in the entry table's order, each with its `kind`, its `name` and every
field and section `CATALOG.md` section 5 gives its kind, by the field names that file uses, a string as its text, a
type descriptor by its tag and arguments, a program as its instructions. A test decodes the compiled version and
compares it with this file entry by entry and field by field; the version's bytes are pinned once that comparison
passes (`SCHEMA.md` section 10.2, rule 6).
