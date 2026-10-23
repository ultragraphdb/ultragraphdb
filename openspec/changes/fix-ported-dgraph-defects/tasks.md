## 1. Deviation tables and guards

- [ ] 1.1 A `DEVIATIONS` table (case name, ledger entry, expected port result) in each golden test that expects the port to differ from Go; the blanket "Go panicked, accept any error" comparisons go
- [ ] 1.2 Workspace `clippy.toml` banning `f64::mul_add` and `f32::mul_add`; one golden S2 vector whose fused and unfused results differ (ledger 11)
- [ ] 1.3 DQL generator records each input's set of distinct outcomes over its runs; the golden test checks the port's outcome is in the set, unstable cases included (ledger 12)

## 2. uid-lists: codec and algo (ledger 1–5, 25–27)

- [ ] 2.1 Feature scenarios for the uid-lists requirements of this change; run red
- [x] 2.2 `Decoder::sought` replaces `Decoder::unpositioned`; callers in codec and algo build through it (4)
- [x] 2.3 `peek_next_base` returns `Option<u64>`; `linear_seek(u64::MAX)` returns the last block (5)
- [x] 2.4 `seek_to_block`: `blocks.get(prev)`, unpack when the target block differs from the one held, `uids.last()` for an empty block; the `# Panics` sections go (3, 4, 27)
- [ ] 2.5 `intersect_sorted_packed` folds every list; `difference_packed` equals `difference` over all blocks; golden comparisons use the plain results (1, 2, 25, 26)

## 3. key-encoding (ledger 6)

- [x] 3.1 Feature scenario; run red
- [x] 3.2 `parse` returns `InvalidFormat` when the type byte is missing; golden `panics` cases expect the error

## 4. scalar-types (ledger 10, 19, Q2, Q4)

- [ ] 4.1 Feature scenarios; run red
- [ ] 4.2 Float to int refuses exactly 2^63; float and bigfloat to datetime refuse values outside int64 seconds, NaN and the infinities with `Float out of int64 range` (10)
- [ ] 4.3 Conversion to string or default refuses invalid UTF-8 with `Value is not valid UTF-8 for type string`; the string value variant holds a `String`
- [ ] 4.4 `marshal_binary(&Val) -> Result<Vec<u8>, TypesError>` replaces the empty-bytes fallbacks in types, facets and dql (19)

## 5. tokenizers (ledger 14, 16, 17, 18, 30, 31)

- [ ] 5.1 Feature scenarios; run red
- [ ] 5.2 The analysis pipeline takes `&str`: `analyze`, `word_segments`, `to_lower`, `nfkc`, the stemmers, `cjk_bigram`; `AnalysisToken` holds a `String` without `tail`; golden cases with invalid UTF-8 input move to deviations or out of the analysis suites (17, 18, 31)
- [ ] 5.3 The segmenter's invalid-byte guard goes (18)
- [ ] 5.4 `to_lower` moves unchanged runes and applies the final-sigma rule on the widening path (16, 30)
- [ ] 5.5 `get_tokenizer_for_lang` gives ngram its language; the wildcard arm becomes an explicit list (14)

## 6. facets (ledger 20)

- [x] 6.1 `FacetValue` replaces the `(Value, ValType)` pair; `to_binary` encodes each variant; the chunker builds `FacetValue` directly
- [x] 6.2 The golden `panic` cases of `facets_to_binary` are covered as not expressible; the other cases pass unchanged

## 7. dql-parsing (ledger 12, 13, 29)

- [ ] 7.1 Feature scenarios; run red
- [x] 7.2 `Object` enum from `NQuad::object()`; value edges take `&api::Value`; an N-Quad with neither object is refused with `Unknown value type for nquad` (13)
- [x] 7.3 `delete_predicate_edge(attr, namespace)` replaces `to_delete_pred_edge` (13, 29)
- [ ] 7.4 Every failing variable and argument reported, in key order, joined by newlines (12)

## 8. mutation-input (ledger 19, 21, 22, 23, 24)

- [x] 8.1 Feature scenarios; run red
- [x] 8.2 `parse_json` refuses non-whitespace after the first value with Go's `after top-level value` text and no longer re-decodes (22)
- [x] 8.3 List items under `predicate@lang` carry the language (23)
- [x] 8.4 `data_format`: the `rdf` or `json` option wins, otherwise the extension (21)
- [x] 8.5 The null-geometry refusal and the JSON number outcomes pass as scenarios (19, 24)

## 9. Later stages' requirements

- [ ] 9.1 Stage 2: the namespace data drop removes `0x04` parts (7); the complete namespace drop removes its schema and type keys durably and writes the initial schema at ts 1 (8); rollups pause across drops
- [ ] 9.2 Stage 3: `LeaseKind` from the API edge, timestamps through Zero's `Timestamps` (9); `after:` past a list's last UID under a filter returns nothing and is excluded from the differential runs (3)
- [ ] 9.3 Stage 5: the bulk loader leases namespaces from the namespace counter (28); a backup checks every string value for valid UTF-8 before writing it

## 10. Close

- [ ] 10.1 The ledger's "The port" column states each entry's remedied behaviour
- [ ] 10.2 Traceability: no scenario of this change pending for the stage-1 groups; `scripts/bdd_counts.sh` reports equal counts
- [ ] 10.3 `make test` green; `make fix` and `make format` clean
