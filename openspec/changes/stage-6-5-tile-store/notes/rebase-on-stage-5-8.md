# Rebase on stage 5.8

Stage 5.8's change (`openspec/changes/archive/2026-11-17-stage-5-8-system-group`) synced its deltas into
`openspec/specs/` on 2026-11-18. Archiving copies a MODIFIED requirement's whole text from this change's delta, so each
requirement below, which this change modifies and stage 5.8 changed, is brought to the archived text first. Then only
this change's own edits differ from it (task 0.7). Every stage 5.8 scenario is kept unless this change
removes it on purpose, and each such removal is stated in the delta.

8 requirements are modified by both. Under each, the scenarios of stage 5.8's archived text that this
change's delta does not hold, compared on 2026-11-17:

- `partitions`, "A storage process hosts partitions over bucket ranges":
  - "The system group forms the placement"
- `role-binaries`, "Each role is its own binary and image":
  - "The frontend binary carries no book"
- `role-binaries`, "Each role runs one runtime per core": every scenario held; the requirement's text is compared
- `role-binaries`, "Readiness, health and shutdown":
  - "The frontend is ready only once the books answer"
  - "The frontend is unready while its system group is unreachable"
- `schema-changes`, "Drops":
  - "Dropping a predicate removes it from the predicate lists"
- `transactions`, "Exactly-once commits through sessions":
  - "A partition purges an entry only after the registry's eviction"
  - "A retry checked in the quiet time stays undecided"
- `wire-protocol`, "Data requests, prepared statements and describe":
  - "Describe names the catalog version it is drawn from"
  - "A client's Describe naming a catalog version is refused"
  - "A Describe continues across frames"
  - "A Describe past the catalog's bound is refused"
- `wire-protocol`, "Roles exchange role commands":
  - "A list larger than a frame continues across frames"
  - "A Scan's `after` is read from the frame"
