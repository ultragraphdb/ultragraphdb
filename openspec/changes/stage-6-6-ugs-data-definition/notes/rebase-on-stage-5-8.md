# Rebase on stage 5.8

Stage 5.8's change (`openspec/changes/archive/2026-11-17-stage-5-8-system-group`) synced its deltas into
`openspec/specs/` on 2026-11-18. Archiving copies a MODIFIED requirement's whole text from this change's delta, so each
requirement below, which this change modifies and stage 5.8 changed, is brought to the archived text first. Then only
this change's own edits differ from it (task 0.16). Every stage 5.8 scenario is kept unless this change
removes it on purpose, and each such removal is stated in the delta.

4 requirements are modified by both. Under each, the scenarios of stage 5.8's archived text that this
change's delta does not hold, compared on 2026-11-17:

- `metadata-group`, "Catalog versions take effect at an epoch boundary":
  - "A catalog version past one frame travels in several"
  - "A catalog version at its cap is held and read"
- `schema-changes`, "Drops":
  - "Dropping a predicate removes it from the predicate lists"
- `wire-protocol`, "Data requests, prepared statements and describe":
  - "Describe names the catalog version it is drawn from"
  - "A client's Describe naming a catalog version is refused"
  - "A Describe continues across frames"
  - "A Describe past the catalog's bound is refused"
- `wire-protocol`, "Roles exchange role commands":
  - "A list larger than a frame continues across frames"
  - "A Scan's `after` is read from the frame"

## What the rebase must settle

Compared on 2026-11-18, scenario by scenario, against `openspec/specs/` as stage 5.8 archived it. Stage 5.9 changes nine
of the same capabilities again (the role's name, the removed relay, the node list), so this change is rebased once,
after stage 5.9 is archived, and the comparison is repeated then.

### 1. Deltas that would undo stage 5.8

Archiving copies a MODIFIED requirement whole, so each stale copy below would replace stage 5.8's text.

| Requirement | What the stale delta does |
| --- | --- |
| `wire-protocol`, "Data requests, prepared statements and describe" | Drops "Describe names the catalog version it is drawn from", "A client's Describe naming a catalog version is refused", "A Describe continues across frames" and "A Describe past the catalog's bound is refused" |
| `metadata-group`, "Catalog versions take effect at an epoch boundary" | Drops "A catalog version past one frame travels in several" and "A catalog version at its cap is held and read" |
| `schema-changes`, "Drops" | Holds "Dropping a predicate holding vectors drops its values" and "A tenant's drop leaves a tenant sharing its run", which stage 5.8 moved to the backlog (items 22 and 21, stages 8 and 7) |
| `wire-protocol`, "Roles exchange role commands" | Drops "A list larger than a frame continues across frames" and "A Scan's `after` is read from the frame", as stage 6.5's delta does; whichever change is archived second repeats the other's edit |

### 2. Defects of this change's own

- **A contradiction with stage 6.5.** This change's `wire-protocol` delta says "A Describe past the client's frame
  continues"; stage 6.5's says it "is refused". Stage 5.8 built "A Describe continues across frames".
- **A rule stage 5.8 already holds.** `schema-changes`, "Drops" adds "A Drop naming a stale catalog version is refused".
  Stage 5.8 built that as the requirement "A partition checks the catalog version a Drop names".
- **MODIFIED requirements the main specifications do not hold.** `openspec validate` passes, and `openspec archive` fails
  on each. This change is stacked on stage 6.5's and is archived after it:
- `system-identity`, "An IRI is the key of the Resource type" (no open change adds it)
- `term-dictionary`, "The dictionary is kept in record runs of kinds 1 to 3" (stage 6.5's change adds it)
- `tokenizers`, "Geo tokens index S2 cells" (no open change adds it)

### 3. Differences that are this change's content

Every MODIFIED requirement whose scenarios differ from the main specifications. Those not named above are this change's
own content; the rebase keeps them and still compares each requirement's text:

- `catalog`, "Entity types"
  - drops: "Preorder numbers follow the hierarchy"
  - adds: "Prefixes follow declaration order"
- `client-endpoint`, "Vector-index declarations wait for stage 8"
  - adds: "An Install declaring a vector index is accepted"
- `metadata-group`, "Catalog versions take effect at an epoch boundary"
  - drops: "A catalog version past one frame travels in several"
  - drops: "A catalog version at its cap is held and read"
  - adds: "A catalog version past one frame travels in continuation frames"
  - adds: "A default entry does not outlive a drop of everything"
- `schema-changes`, "Drops"
  - drops: "Dropping a predicate removes it from the predicate lists"
  - adds: "Dropping a predicate leaves none of its entries"
  - adds: "Dropping a predicate holding vectors drops its values"
  - adds: "Dropping a tenant's everything survives a restart"
  - adds: "A tenant's drop leaves a tenant sharing its run"
  - adds: "Dropping everything removes the default entries"
  - adds: "A declared predicate is not dropped in a governed tenant"
  - adds: "A Drop naming a stale catalog version is refused"
  - adds: "A Drop removes history with its tenant"
- `schema-storage`, "Writes to an unknown predicate create its schema"
  - adds: "A governed tenant records nothing for a refused predicate"
  - adds: "An open type records its undeclared attribute"
- `term-dictionary`, "IRIs and blank nodes get system IDs"
  - adds: "An IRI of a declared type's instance maps to that type's prefix"
- `wire-protocol`, "Data requests, prepared statements and describe"
  - drops: "Describe names the catalog version it is drawn from"
  - drops: "A client's Describe naming a catalog version is refused"
  - drops: "A Describe continues across frames"
  - drops: "A Describe past the catalog's bound is refused"
  - adds: "A Describe past the client's frame continues"
  - adds: "Rows come partition by partition"
- `wire-protocol`, "Roles exchange role commands"
  - drops: "A list larger than a frame continues across frames"
  - drops: "A Scan's `after` is read from the frame"
  - adds: "A row larger than a frame continues across frames"
  - adds: "A Scan's range bytes are read from the frame"
  - adds: "A range's fields are checked against its space"
  - adds: "A resume key rides only on a range that stopped"
  - adds: "Widths stay within a range"
