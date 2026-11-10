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
                         checksum and digest; the data tiles placed in built order on a 16-extent grid of 128 KiB
                         extents, then a checkpoint's manifest leaf, root and free set
vectors/tiles/*.tile     the tiles' bytes
vectors/grid.json        sections 3.2, 11.3, 12 and 13: the derivation version, an allocation script, the extents the
                         checkpoint leaves allocated, and the superblock of that checkpoint with its checksum and identity
vectors/tiles/*.block    the superblock's four copies
vectors/decisions.json   sections 12.4, 13.2, 16.2, 16.3, 17.1 and 17.2: output cuts, directory lookups, the queue's order,
                         stream choices, checkpoint positions and superblock selection over four blocks
vectors/refusals.json    sections 4 and 19.4: valid bytes broken by one rule each, with the refusal a decoder names
```

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
| `refusals.json` | — | — | each case is refused with `refusal`: a tile's first broken rule in the order of section 6.5, a section's in its encoding's order of section 4, a superblock copy's in section 17.2's; the control case is accepted; `Misdirected` is read through a reference whose checksum is `0000000000000001` |

A vector names its tiles' settings; where it says "defaults", they are section 3.1's.
