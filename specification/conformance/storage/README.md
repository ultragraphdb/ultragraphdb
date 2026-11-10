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
vectors/encodings.json   section 4: FOR and DELTA with the choice between them, BITMAP, BYTES, FIXED, FILTER with probes
vectors/sortable.json    sections 1 and 5: sortable values with the orders and equalities they keep, term encodings,
                         tokens (a long one hashed) and esc with the tuple order it keeps
vectors/tiles.json       sections 6 to 13: each tile's input, every header field, its fence, tile key, address, length,
                         checksum and digest; the data tiles placed in built order on a 64-extent grid of 128 KiB
                         extents, then a checkpoint's manifest leaf, root and free set
vectors/tiles/*.tile     the tiles' bytes
vectors/grid.json        sections 3.2, 11.3, 12 and 13: the derivation version, an allocation script, the extents the
                         checkpoint leaves allocated, and the superblock of that checkpoint with its checksum and identity
vectors/tiles/*.block    the superblock's four copies
vectors/decisions.json   sections 12.4, 13.2, 16.2, 16.3, 17.1 and 17.2: output cuts, directory lookups, the queue's order,
                         stream choices, checkpoint positions and superblock selection over four blocks
vectors/refusals.json    sections 4 and 19.4: valid bytes broken by one rule each, with the refusal a decoder names
vectors/kinds.json       sections 7 to 9: one tile of every kind tiles.json leaves out (adjacency kinds 2, 4 and 5,
                         column kinds 2 and 3, record kinds 2 to 8, 10 to 15 and 18, a versioned record run)
vectors/compaction.json  sections 11.2 and 16.3: a hub row halved until it fits, and a column chunk's overlay folded
                         with its conservation
vectors/engine.json      sections 12 to 17: the reference engine's run, below
vectors/tiles/engine_grid.blocks  the run's final grid, per nonzero block its index as a u32 and its 4,096 bytes
```

## The reference engine

`generate_vectors.py` holds a reference engine written from sections 12 to 17: one relation's forward (kind 1) and
reverse (kind 3) families in one tenant, graph and bucket group, class M, under small settings (`engine.json`'s
`settings`). It applies 220 scripted rounds (`rounds`: each a position, a logged fold horizon, a frame length and
commits of inserts and deletes, valid by construction) through the due rules, the queue, units, rewrites with merge,
fold, cut, halving and the next-tile merge, placement and streams, the cleaner, and checkpoints with their drain,
manifest, free set and superblock. A conforming engine fed the same rounds under the same settings reproduces:

- `events`: every rewrite in completion order, its boundary, its deciding boundary, its inputs' tile keys and its
  outputs' tile keys, addresses, lengths and checksums;
- `checkpoints`: each checkpoint's position, log bytes, sequence, superblock checksum, identity, tile and leaf counts,
  the extents its free set marks, and the XXH3-64 of the whole grid file at that moment;
- `final`: the grid file after the last round, as `engine_grid.blocks`;
- `crash`: a second replica crashed after round 35, recovered from its grid alone (section 17.2), replayed rounds 31
  onward, and its later checkpoints and final grid equal the first replica's.

The rounds name subject slots directly, since slot maps are record runs outside the model; it exercises no statement,
inline column, column chunk, drop, purge, build, pressure (reason 5), cold stream or cleaner move, whose rules the
vectors above cover one by one.

## What a conforming implementation checks

| Vectors | Encode | Decode | Refuse |
|---|---|---|---|
| `encodings.json` | Each case's input encodes to `hex`; for packed integers `encoding` is the code the choice rule picks, and `for_hex` and `delta_hex` are both candidates | `hex` decodes to the input | — |
| `encodings.json`, filters | `m`, `k` and `hex` from the keys and `bits_per_key` | each probe answers `may_be_present` | — |
| `sortable.json` | each sortable value, term, token and `esc` case encodes to `hex` | `hex` decodes to the value | — |
| `sortable.json`, orders | every list in `ascending` is strictly ascending as bytes; every list in `equal` encodes equally; `esc.ascending_indices` orders the joined cases as bytes and as tuples alike | — | — |
| `tiles.json` | the input with every `header` field encodes to the `.tile` file byte for byte; `checksum`, `digest`, `fence_hex` and `tile_key_hex` follow; the data tiles placed in `built` order, then the leaf, the root and the free set in stream 0, land at `address` | the file decodes to the input, and its digest recomputes from its entries | — |
| `grid.json` | the derivation version from the settings; each placement's `address` from the script, in order; the four superblock copies from `superblock.input` | the copies decode to the input and to one checksum; `identity` recomputes from section 11.3 | — |
| `decisions.json` | each cut, lookup, queue order, stream, checkpoint position list and superblock selection is what the sections give for its input | — | the selections whose result is a refusal |
| `kinds.json` | each input with its `header` encodes to its `.tile` file; `fence_hex` and `tile_key_hex` follow | the file decodes to its input | — |
| `compaction.json` | the halving's pieces from its entries and settings; the folded chunk from the input chunk at `horizon`, with `added_hex` and `removed_hex` | the conservation of section 11.2 holds | — |
| `engine.json` | the events, checkpoints and final grid from the rounds and settings | recovery from the grid alone, then replay, reproduces them | — |
| `refusals.json` | — | — | each case is refused with `refusal`: a tile's first broken rule in the order of section 6.5, a section's in its encoding's order of section 4, a superblock copy's in section 17.2's; the control case is accepted; `Misdirected` is read through a reference whose checksum is `0000000000000001` |

A vector names its tiles' settings; where it says "defaults", they are section 3.1's.
