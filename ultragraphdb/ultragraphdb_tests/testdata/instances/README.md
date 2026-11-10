# The instance corpus

Instance sets for Gen5 and the aviation schema that hold or break each stage 6 rule of `SCHEMA.md` section 8, with
the refusal `SCHEMA.md` section 8.7 numbers, written from the specification alone. The acceptance tests (task 12.3)
and the UQL parity test (task 10.5) run each case against a cluster of one partition holding the schema.

## Layout

```
gen5/<UGSnnnn>-<slug>/case.json      a case and the files it names
aviation/<UGSnnnn>-<slug>/case.json
gen5/holds-<slug>/case.json          a case every step of which commits
```

## A case

```json
{
  "schema": "gen5",
  "rule": "UGS1023",
  "name": "a revision to preferred breaks one preferred instance per subject and type",
  "steps": [
    { "insert": "setup.nq", "expect": "commit" },
    {
      "insert": "revise.nq", "delete": "revise-old.nq",
      "uql": ["update IsAbout(information: Document(id = \"DOC:d1\"), topic: Concept(id = \"CON:c2\")) { rank: preferred }"],
      "expect": { "refused": "UGS1023" }
    }
  ],
  "view": "view.nq"
}
```

- `schema` is `gen5` (`specification/schema/deepbrain-gen5.ugs`) or `aviation` (`SCHEMA.md` section 16), installed into
  a fresh tenant 0 before the first step.
- Each step is one read-write transaction: its `delete` quads removed and its `insert` quads written, both N-Quads
  files, then its `operations` decided, `{ "kind": "delete" | "retract", "iri": "..." }`, Operation kinds 5 and 6;
  then the commit. `expect` is `"commit"` or `{ "refused": "UGSnnnn" }`; a refused step writes nothing.
- `uql`, where the subset of `QUERY-ULTRA-QL.md` section 4.1 expresses the step, is the same step as UQL requests, one
  Query each, in one transaction. A step the subset cannot express carries no `uql`, and the case runs as quads alone.
- A blank node label names one instance through the whole case: the first step writing it creates the instance, and
  the harness names it by the handle IRI its partition minted in every later step. An operation's `iri` may be such a
  label from an earlier step, and a label inside a triple term is substituted as one outside it is.
- `steps` may give `"generate": { "from": 1, "to": 4096, "insert": "line.nq" }` in place of `insert`: the template's
  lines written once per `i` from `from` to `to` inclusive, `{i}` replaced by `i` and `{j}` by `i − 1`, in one
  transaction.
- `view`, optional, is the RDF view (`SCHEMA.md` section 11.2) of the tenant after the last committed step, compared
  as RDF datasets up to blank node renaming, with the built-in `asserted` and `live` triples left out, since their
  values are epochs, and the derived names' triples left out, as `R` of `SCHEMA-APG.md` definition 17.1 leaves them.
- A case reads at one partition: where a rule would reach a participant in another, the case places it in its
  anchor's partition, and code 23 is tested by `ultragraphdb_cluster_partition` (design decision 10).
