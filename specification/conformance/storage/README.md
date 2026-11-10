# The storage conformance vectors

The vectors hold inputs and the exact bytes `../../storage/STORAGE.md` gives for them, written by
`generate_vectors.py` from the specification alone, never from an implementation's output. The tile store and its
simulator are built against them, and a second implementation is held to the same files. `BUILD.bazel` exports every
file as the filegroup `//specification/conformance/storage:vectors`.

Regenerate from this directory, with Python 3.12 and its `xxhash` 3.6.0, the generator the protocol's golden bodies
use:

```
python3 -I generate_vectors.py .
```

The generator is deterministic: two runs write the same bytes. A change to `STORAGE.md` that changes a byte changes the
generator in the same commit, and the derivation version's format or algorithm (`STORAGE.md` section 3.2) with it.

## Layout

```
vectors/encodings.json   section 4: FOR and DELTA with the choice between them, BITMAP, BYTES, FIXED up to
                         value_bytes_max wide, FILTER with probes
vectors/sortable.json    sections 1 and 5: sortable values with their comparison forms and whether they are small, the
                         orders and equalities their comparison forms keep, term encodings, tokens (range tokens of a
                         comparison form, a measure in its canonical unit, the generic layout's range tokens by a
                         literal's datatype with integers outside Int64 taking none, count tokens, a long one hashed)
                         with the orders and equalities tokens keep, and esc with the tuple order it keeps
vectors/tiles.json       sections 6 to 13: each tile's input, every header field, its fence, tile key, address, length,
                         checksum and digest; the data tiles placed in built order on a 64-extent grid of 128 KiB
                         extents, then a checkpoint's manifest leaf, root and free set
vectors/tiles/*.tile     the tiles' bytes
vectors/grid.json        sections 3.2, 11.3, 12 and 13: the derivation version, an allocation script, the extents the
                         checkpoint leaves allocated, and the superblock of that checkpoint with its checksum and identity
vectors/tiles/*.block    the superblock's four copies
vectors/decisions.json   sections 12.4, 13.2, 16.2, 16.3, 16.6, 17.1 and 17.2: output cuts, directory lookups over
                         slot-pair fences, the queue's order, stream choices, the reserve, the cleaner's due and move
                         rule, checkpoint positions with and without flagged rounds, and superblock selection over four
                         blocks
vectors/refusals.json    sections 4, 6.3, 7.3, 9.1, 10 and 19.4: valid bytes broken by one rule each, with the refusal a
                         decoder names: among them a FILTER section while filter_bits_per_key is 0, a deleted version
                         not above its created one in an adjacency block and in a record run, and a free set whose
                         bit 0 is clear
vectors/kinds.json       sections 7 to 9: adjacency kinds 2, 4 and 5 and a compiled kind 3 ordered by an inline
                         column, column kinds 2 and 3, and a run of every record kind 1 to 20 (16 and 17 name tiles.json's
                         manifest leaf and root), kind 14 twice, a drop's marker and a catalog version's naming
                         operation 6, the additive kinds 7, 9 and 19 with their folds
vectors/compaction.json  sections 11.2 and 16.3: a hub row halved until it fits, and a column chunk's overlay folded
                         with its conservation
vectors/engine.json      sections 12 to 17: the reference engine's run, below
vectors/tiles/engine_grid.blocks  the run's final grid, per nonzero block its index as a u32 and its 4,096 bytes
```

## The reference engine

`generate_vectors.py` holds a reference engine written from sections 12 to 17: one relation's forward (kind 1) and
reverse (kind 3) families of the generic layout in one tenant, graph and bucket group, class M, under small settings
(`engine.json`'s `settings`). Forward rows are subject slot pairs of two types, reverse rows the objects' term
encodings. It applies 220 scripted rounds (`rounds`: each a position, a logged fold horizon, its `flags`, a frame length
and commits of inserts and deletes, each subject a type prefix and a slot) through the due rules, the queue, units,
rewrites with merge, fold, cut, halving and the next-tile merge, placement and streams, the reserve and the cleaner's
due rule, and checkpoints with their drain, manifest, free set and superblock. Rounds 25 and 150 are empty and flagged
`checkpoint` (`PROTOCOL.md` section 12.1, `flags` bit 0), so each is a checkpoint position (section 17.1); round 151
crosses a byte threshold right after, so its boundary frees what checkpoint 150 released and then checkpoints again.
The script is valid by construction: before each round the engine checks admission (section 15.6) as the primary would,
the delta with the round's changes at or below the watermark and free extents at or above the reserve with them
counted, and `admission` records the delta, free extents and reserve it found. A conforming engine fed the same rounds
under the same settings reproduces:

- `events`: every rewrite in completion order, its boundary, its deciding boundary, its inputs' tile keys and its
  outputs' tile keys, addresses, lengths and checksums;
- `checkpoints`: each checkpoint's position, log bytes, sequence, superblock checksum, identity, tile and leaf counts,
  the extents its free set marks, the extents its step 6 frees, which become free at the next round's boundary before
  its placements, and the XXH3-64 of the whole grid file at that moment, which assumes freed extents read as zero,
  punched, and replica number 1 in the superblock;
- `final`: the grid file after the last round, as `engine_grid.blocks`;
- `crash`: a second replica crashed after round 30, recovered from its grid alone (section 17.2) to the flagged
  checkpoint at 25, replayed rounds 26 onward, and its later checkpoints and final grid equal the first replica's.

`free` counts free extents only, so a released extent counts once a checkpoint has freed it. The run stays far from
`GridFull`: at most 8 of 63 data extents are in use, so free extents never fall below 55 against a reserve of at most
10, and the cleaner is never due, whose due and move rule `decisions.json` covers instead. The rounds name subject
slot pairs directly, since slot maps are record runs outside the model. The engine leaves out statements, inline
columns, column chunks, record runs, drops, purges, builds, count degrees (the round's derivations section), session
evictions, Ingest bodies, pressure (reason 5), the cold stream, cleaner moves, and the primary's own decision to flag a
round: a flagged round of the script is taken as a checkpoint position whatever the grid holds, which is all the store
does with the flag. The vectors above cover those rules one by one.

`Geo` values (tag 29) are canonicalised by an RFC 8785 serialiser in the generator that handles strings, integers,
booleans, null, arrays and objects; the samples hold no fractional number, whose canonical form needs ECMAScript's
number formatting. Every token in `sortable.json` takes its identifier byte as an input of the vector, since the
`tokenizers` capability's registry assigns it: the token's bytes after it are what the vector fixes. A generic range
token names its tokenizer, one per sortable type (`int`, `float`, `decimal`, `datetime`, `bool`), and tokens of two
types differ in that byte even where their comparison forms are equal. A generic
`xsd:float` is parsed as a double, `"0.1"^^xsd:float` giving the token of `"0.1"^^xsd:double`; a decimal with more than 18
fraction digits or outside `Int128` at scale 18, like an integer outside `Int64`, has `hex` null, taking no range token;
a count of 0 takes its token.

## What a conforming implementation checks

| Vectors | Encode | Decode | Refuse |
|---|---|---|---|
| `encodings.json` | Each case's input encodes to `hex`; for packed integers `encoding` is the code the choice rule picks, and `for_hex` and `delta_hex` are both candidates | `hex` decodes to the input | — |
| `encodings.json`, filters | `m`, `k` and `hex` from the keys and `bits_per_key` | each probe answers `may_be_present` | — |
| `sortable.json` | each sortable value, term, token and `esc` case encodes to `hex`; a token case whose `hex` is null takes no token, an integer outside `Int64`; each sortable value's `comparison_hex` is its comparison form and `small` whether it may sit inline | `hex` decodes to the value | — |
| `sortable.json`, orders | every list in `ascending` is strictly ascending by comparison form; every list in `equal` has one comparison form, two temporal values of one instant and different offsets among them, whose stored encodings differ; every list in `token_ascending` is strictly ascending as bytes and every list in `token_equal` has one token; `esc.ascending_indices` orders the joined cases as bytes and as tuples alike | — | — |
| `tiles.json` | the input with every `header` field encodes to the `.tile` file byte for byte; `checksum`, `digest`, `fence_hex` and `tile_key_hex` follow; the data tiles placed in `built` order, then the leaf, the root and the free set in stream 0, land at `address` | the file decodes to the input, and its digest recomputes from its entries | — |
| `grid.json` | the derivation version from the settings; each placement's `address` from the script, in order; the four superblock copies from `superblock.input` | the copies decode to the input and to one checksum; `identity` recomputes from section 11.3 | — |
| `decisions.json` | each cut, lookup, queue order, stream, reserve, cleaner decision, checkpoint position list and superblock selection is what the sections give for its input | — | the selections whose result is a refusal |
| `kinds.json` | each input with its `header` encodes to its `.tile` file; `fence_hex` and `tile_key_hex` follow; an additive run's `fold_hex` is its records' fold | the file decodes to its input | — |
| `compaction.json` | the halving's pieces from its entries and settings; the folded chunk from the input chunk at `horizon`, with `added_hex` and `removed_hex` | the conservation of section 11.2 holds | — |
| `engine.json` | the admission figures, events, checkpoints and final grid from the rounds and settings | recovery from the grid alone, then replay, reproduces them | — |
| `refusals.json` | — | — | each case is refused with `refusal`: a tile's first broken rule in the order of section 6.5, a section's in its encoding's order of section 4, a superblock copy's in section 17.2's; the control case is accepted; `Misdirected` is read through a reference whose checksum is `0000000000000001`; a case naming `settings` is read under those, the defaults otherwise |

A vector names its tiles' settings; where it says "defaults", they are section 3.1's.
