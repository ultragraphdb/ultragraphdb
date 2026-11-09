# Group 10b: the TOML subset

One crate, `ultragraphdb_tool/ultragraphdb_tool_toml`, with no external dependency: the parser of the settings'
TOML subset, its document, the typed reads and the final unread-key check, and `TomlError`.

## Deviation from the design's crate table: no `ultragraphdb_error_toml`

The design and the task heading first named an error crate, `ultragraphdb_error_toml`. AGENTS.md gives an error a crate of its
own only when more than one crate raises it, and `TomlError` is raised by `ultragraphdb_tool_toml` alone (the roles
map it into their own error). It therefore lives in `ultragraphdb_tool_toml/src/errors/toml_error/`, as `TlsError`
lives in `ultragraphdb_transport_tls`. The error crate was written first (phase 1) and moved in before phase 2 was
recorded. Its folder was never tracked by git. An emptied member folder under `ultragraphdb_error/*` breaks
`cargo metadata` and so Bazel's crate extension, so its leftover manifest, BUILD file and module stubs were moved out of
the workspace, not left in place. Its `[workspace.dependencies]` line is gone. `design.md`'s table and the task heading
already name the one crate.

`TomlError` does not convert from `InvariantError`: the parser calls no `invariant(...)`, because none of its checks
guards against corrupting stored or sent data, so an `Invariant` variant would be unreachable. Its internal checks are
`debug_assert!`s.

## 10b.1 Scenarios

All eleven scenarios of `settings-toml`, verbatim, in `features/settings_toml/settings_toml.feature`, bound by the
crate's `bdd` target (cucumber, `fail_on_skipped`). "Arbitrary bytes never panic" runs a hermetic stand-in for the
fuzzer: 20,000 inputs from a fixed xorshift64 seed, half arbitrary bytes biased to TOML's characters and half valid
documents with bytes replaced, inserted and cut. Each input is accepted and re-read alike, or refused with a typed
error. The fuzz target itself is phase 5's.

## 10b.2 The toml-test suite

- Source: https://github.com/toml-lang/toml-test, tag **v1.6.0** (annotated tag object
  `65a4da1357c5b38235982f1320bc8173929d8d24`), commit **`369cc3ae275fd76b6e21780ebf87609e5b2a951a`**
  (2025-04-15). v1.6.0 is the last v1 release; v2.0.0 moved the default to TOML 1.1. The suite's own list
  `tests/files-toml-1.0.0` chose the TOML 1.0 documents. MIT, `COPYING` vendored beside the documents.
- The suite was fetched by `git clone --depth 1` into an empty scratch directory. Nothing from it was executed.
- Vendored under `testdata/toml-test/`, at the suite's own paths: all **371** invalid documents of the 1.0 list, and
  **38 of its 185** valid documents, those within the subset, each with its expected JSON. Provenance and the selection
  rule are in `testdata/toml-test/SOURCE.txt`.
- The selection was made by a Python script independent of the parser. A document was taken when its JSON holds only
  string, integer and bool values, at the root or one table deep, and every line matches the subset's grammar as
  regular expressions. The 41 left out on their text alone were reviewed by hand; the ad hoc check below confirms
  each of the 147 left out uses a feature outside the subset.
- `testdata/fuzz_seeds/document/` holds the 38 valid documents again, as the fuzz target's seeds.
- Ad hoc check, not kept in the suite: each of the 147 valid documents outside the subset was parsed through a
  temporary test. Every one was refused as `Unsupported`, none parsed and none was refused as `Invalid`: arrays 26,
  arrays of tables 11, binary 1, dates and times 17, dotted keys 32, floats 8, hexadecimal 2, infinities and NaN 3,
  inline tables 12, literal strings 6, multi-line strings 13, quoted keys 16. The temporary files were moved out
  afterwards.

## Phases

### 1. Surface

`TomlDocument` (`parse`, `tables`, `string`, `integer`, `boolean`, `finish`, and the four limits as associated
constants), `TomlTable`, `TomlEntry` and `TomlValue` (`kind`). `TomlError(pub TomlErrorEnum)` has six variants:
`Unsupported`, `Invalid`, `Limit`, `Missing`, `WrongKind`, `UnreadKey`, with one constructor each. Beside it are
`TomlFeature` (the 13 features outside the subset), `TomlInvalid` (the 14 rules of TOML 1.0 a line can break),
`TomlLimit` and `TomlKind`, each with `Display`. Every body is a bare `unimplemented!()`. **By the user's correction,
the stubs carry no `#[expect(clippy::unimplemented, ...)]`.** The workspace's `unimplemented = "warn"` is deliberate:
the warnings stood through phases 2 and 3, and in phase 4 zero `unimplemented` warnings showed that no body was
overlooked. `bazel build` of the crate passed.

### 2. Suite

- Unit tests (`tests/`, mirroring `src/`):
  - every error variant built through its constructor, with distinct values;
  - every `Display` text;
  - the structure: an empty document, comment-only and blank lines, root keys, empty tables, the bare-key alphabet,
    whitespace, comments after values and headers, LF, CRLF and no final newline, a key repeated across tables;
  - every escape and invalid ones, `\u` and `\U` naming a surrogate or a code point past U+10FFFF;
  - integers with signs and underscores, `i64::MIN` and `i64::MAX` and one past each;
  - each of the 13 unsupported features on its own line, with the TOML 1.0 specification's examples (45 lines);
  - each of the 14 invalid rules;
  - each limit at its bound and one past it;
  - the typed reads, `finish` and the order in which unread keys are reported;
  - the 38 valid toml-test documents against their JSON, and re-read alike; the 371 invalid ones refused;
  - the seeds parse.
- Expected values come from the TOML 1.0 specification (escapes, the i64 bounds as 2^63 - 1 and -2^63) and the
  toml-test suite's JSON (compared through `serde_json`, a dev-dependency). Error texts are literals written from the
  spec.
- Corner cases enumerated in advance, as the task lists them: an empty document, a comment-only line, a root key,
  i64 bounds ±1, every escape and invalid ones, a surrogate `\u`, each unsupported feature on its own line, each
  limit and one past it. Added to these: CR handling, CRLF at the line limit, the document limit before UTF-8
  checking, a key count spread across tables.
- Run against the surface, under Bazel:
  - unit: **67 tests, 66 failed**, every failure at an `unimplemented!()` of the crate. The one that passed,
    `test_the_limits_are_those_of_the_spec`, reads only the associated constants, which are declarations.
  - BDD: **11 scenarios, 11 failed**, each at `not implemented`.

### 3. Audit

Deviation, as in stage 5's group 1: the audited implementation is the one kept, with each defect applied alone by a
script, the crate's Bazel tests run, and the file restored, rather than a throwaway implementation rewritten.

| Defect | Caught by |
|---|---|
| D01 document limit off by one | `test_a_document_of_64_kib_parses_and_one_byte_more_is_refused` |
| D02 line limit off by one | `test_a_line_of_4_kib_parses_and_one_byte_more_is_refused`, the document-limit test |
| D03 table limit off by one | `test_256_tables_parse_and_one_more_is_refused` |
| D04 key limit off by one | `test_4096_keys_parse_and_one_more_is_refused_in_every_table_together` |
| D05 key count restarted at a second header | **missed at first**; the key-limit test split keys between the root table and one table only. It now spreads them over the root table and two tables, and catches it |
| D06 CR stripped from a last line without LF | the invalid suite (`control/bare-cr.toml`), unit and scenario |
| D07 control characters allowed in comments | `test_a_control_character_other_than_tab_is_invalid`, the invalid suite |
| D08 control characters allowed in strings | the same |
| D09 `\b` resolved to BEL | `test_every_escape_resolves_to_its_character`, the valid suite |
| D10 `\f` resolved to LF | the same |
| D11 `\U` reading four digits | the escape, surrogate and invalid-escape tests, the invalid suite |
| D12 a surrogate read as U+FFFD | `test_an_escape_naming_a_surrogate_or_past_the_last_code_point_is_invalid`, the invalid suite |
| D13 leading-zero guard removed | `test_an_integer_with_a_leading_zero_is_invalid`, the invalid suite |
| D14 `__` allowed | `test_a_malformed_integer_is_invalid`, the invalid suite |
| D15 digits accumulated above zero | 15 unit tests and 3 scenarios |
| D16 the sign dropped | 16 unit tests and 3 scenarios |
| D17 duplicate key accepted | the duplicate-key test and scenario, the invalid suite, the fuzz scenario |
| D18 duplicate table accepted | `test_a_table_with_two_headers_is_invalid_on_the_second`, the invalid suite |
| D19 a table over a root key accepted | `test_a_table_named_as_a_root_key_is_invalid` |
| D20 a dotted key refused as malformed | `test_each_unsupported_feature_is_refused_by_name_and_line` |
| D21 floats told before times (`00:32:00.999999`) | the same |
| D22 the hex prefix not told | the same |
| D23 trailing characters ignored | `test_anything_but_a_comment_after_a_value_or_header_is_invalid`, the invalid suite |
| D24 a header's end of line unchecked | the same |
| D25 lines counted from 0 | 20 unit tests and 3 scenarios |
| D26 `finish` checking the root table only | 3 read tests, the misspelling scenario |
| D27 a wrong-kind read taking the setting | `test_a_read_of_the_wrong_kind_takes_nothing` |
| D28 a read ignoring the table | `test_an_absent_key_or_table_is_missing`, the re-read test, the fuzz scenario |
| D29 a root setting printed with a dot | `test_a_root_setting_is_named_by_its_key_alone` |
| D30 the multi-line check removed | the unsupported-feature test |
| D31 `nan` not told | the same |
| D32 tab not whitespace | the blank-line and whitespace tests |
| D33 root keys not kept for headers | `test_a_table_named_as_a_root_key_is_invalid` |
| D34 a quoted header refused as malformed | the unsupported-feature test |
| D35 a bare key taking a dot | the unsupported-feature test, the invalid suite |
| D36 an unterminated string accepted | `test_a_string_not_closed_on_its_line_is_invalid`, the invalid suite |
| D37 an out-of-range integer wrapping | the out-of-range and i64-bound tests |

37 planted, 37 caught once D05's gap was closed. Input variety: every refusal test puts its line on line 2 or later
after a valid line, limits differ from line numbers, and constructor tests give every field a distinct value.

### 4. Implementation

`TomlParser` (`src/types/toml_parser/`) reads the document in one pass:
- It refuses a document over 64 KiB before anything else.
- It splits the bytes at line feeds, inclusively. A CR is stripped only before its LF.
- Each line is bounded at 4 KiB and checked as UTF-8 on its own, because a line feed never occurs inside a UTF-8
  sequence.
- Each line is read from its start to its end by functions of the rest of the line (`src/utils/`): character classes,
  basic strings with their escapes, and values told by their first characters.
- Duplicates are found through `BTreeSet`s of the names defined so far.

There is no recursion, no indexing and no `unwrap`. Every loop runs over a finite collection or a fixed count.

The first full run found two real defects. A CR was stripped from a final line that no LF followed, so the suite's
`invalid/control/bare-cr.toml` parsed. `_1` was refused as a malformed value rather than a malformed integer. The
implementation changed; no test did.

- Bazel: unit **67 passed**, BDD **11 scenarios (22 steps) passed**.
- Zero `unimplemented` warnings.
- Clippy over the workspace table: clean.
- The commands of `make format` and `make fix` (`buildifier`, `cargo fmt`, `cargo clippy --fix -- -Wclippy::all`),
  run scoped to this crate and its fuzz package, leave every file unchanged and report nothing. They were scoped
  rather than run workspace-wide because another group was editing other crates at the same time.

By the user's correction, the four held-back lints were not run.

### 5. Fuzz target

- Package: `fuzz/ultragraphdb_tool_toml`, target `document`. Any bytes are fed to `TomlDocument::parse`, which must
  not panic.
- An accepted document reads back alike: each setting's typed read answers its value and `finish` passes.
- The document is written again as TOML of the subset by the target's own writer. Within the limits, it parses to
  the same rows.
- Seeds: `testdata/fuzz_seeds/document/`.
- `cargo +nightly fuzz build` passed.
- A 20-second smoke run, not a campaign: 2,801,009 executions, coverage 1058, no failure. Group 12 runs
  `scripts/fuzz.sh`.

## Decisions the spec left open (for the user's review)

1. **Whitespace:** spaces and tabs are accepted wherever TOML 1.0 allows them: indentation, around `=`, inside
   `[ table ]`, before a comment.
2. **Line endings:** LF and CRLF. A CR anywhere else, the last line included, is refused as a control character.
3. **A BOM is refused** (as a malformed key). TOML 1.0's grammar has none, and the suite's 1.0 list has no valid BOM
   document.
4. **The limits:**
   - The line limit counts a line's bytes without its LF or CRLF.
   - The table limit counts tables with a header (256), not the root table.
   - The key limit counts every table together (4,096).
   - The document limit is checked first, so 64 KiB plus one byte of anything is a `Limit` error.
   - `Limit` carries the limit and its bound, not a line.
5. **The first refusal in reading order wins.** A duplicate key is reported when its key is read, before its value.
6. **A table named as a root key** is refused as `DuplicateKey(name)`, since a table is a key of the root table.
7. **Classification of bare values:**
   - `true`/`false`; then `inf`/`nan` with an optional sign.
   - A token starting with a digit, `+`, `-` or `_` is a number: `0x`/`0o`/`0b` name their feature; four digits and
     `-`, or two digits and `:`, a date or time; `.`/`e`/`E` a float; anything else must be a valid decimal integer.
   - Any other token is a malformed value.
8. **Reads:**
   - A setting is taken only by a read of its kind. A read of the wrong kind leaves it untaken, so `finish` would
     report it too.
   - An absent table reads as a missing key.
   - The root table is named `""`.
   - `finish` reports the first untaken setting in the document's order.
9. **Document API:** only the required accessors were built, with no optional form, so a role reading an optional
   setting matches on `Missing`. Entries carry no line numbers.

## Left for the user

- The error crate's leftovers (its Cargo.toml, BUILD.bazel, `lib.rs` and the two `mod.rs` stubs) sit outside the
  repository, in the session's scratchpad, and can be discarded.
- Coverage (`scripts/coverage.sh`, `cargo llvm-cov`) was not run: the group's brief allowed Cargo only for fuzz,
  format and fix.
