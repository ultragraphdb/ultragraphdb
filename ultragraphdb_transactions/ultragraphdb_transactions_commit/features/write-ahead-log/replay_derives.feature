# The replay scenarios of the write-ahead-log capability's requirement "A round's record is logical" that name a
# partition's references and registrations, verbatim from
# openspec/changes/stage-5-5-three-role-refactor/specs/write-ahead-log/spec.md: the pipeline commits through the
# write-ahead log on simulated storage into a changeset applier over the simulated store, a reference adopted and marked
# as a home's reply names them and a registration accepted as the home accepts it, with no second partition, and the
# log is replayed into an empty store.

Feature: A round's record is logical

  # Requirement: A round's record is logical

  Scenario: Replay derives the same keys
    When a round holding a first reference to an entity another partition owns, with its registration mark and the copied ID-to-IRI entry, is applied, the store is discarded, and the round's record is replayed into an empty store
    Then the store holds exactly the keys and values it held after the first apply, the mark's generation and the copied entry included

  Scenario: Replay derives a home's referrer-set deltas
    When a round holding only two registrations a home accepted is applied, the store is discarded, and the record is replayed
    Then the referrer set holds both members, each at the commit version the record lists for its registration
