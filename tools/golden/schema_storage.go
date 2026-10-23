// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_schema's storage: the schema and type entries Dgraph writes
// (schema_storage_entries), and what schema.Load and schema.LoadFromDb read back from a Badger store after a
// sequence of writes (schema_storage_loads).
//
// The entries are those of worker/mutation.go's updateSchema and updateType, which are unexported and written
// here as Dgraph writes them: the key, proto.Marshal of the update, the user meta BitSchemaPosting and the
// discard flag, read back from Badger. Every predicate and type of the schema corpus parsed in namespace 0 and the
// complete initial schema and types of namespaces 0 and 5 are written. The loads run Dgraph's own Init, Delete,
// DeleteType, Load and LoadFromDb over an in-memory managed Badger with Dgraph's namespace offset.

import (
	"context"
	"encoding/hex"
	"flag"
	"fmt"
	"math"
	"sort"

	"github.com/dgraph-io/badger/v4"
	"google.golang.org/protobuf/proto"

	"github.com/dgraph-io/dgraph/v25/posting"
	"github.com/dgraph-io/dgraph/v25/protos/pb"
	"github.com/dgraph-io/dgraph/v25/schema"
	"github.com/dgraph-io/dgraph/v25/x"
)

func init() {
	register("schema_storage_entries", "ultragraphdb_core_schema", genSchemaStorageEntries)
	register("schema_storage_loads", "ultragraphdb_core_schema", genSchemaStorageLoads)
}

// storageEntryJSON is one stored version as Badger returns it.
type storageEntryJSON struct {
	Key      string `json:"key"`
	Value    string `json:"value"`
	UserMeta uint8  `json:"user_meta"`
	Discard  bool   `json:"discard"`
}

// storageEntriesCase holds the entries of every predicate and type one text parses into, in parse order.
type storageEntriesCase struct {
	Input   string             `json:"input"`
	Initial int64              `json:"initial"`
	Preds   []storageEntryJSON `json:"preds"`
	Types   []storageEntryJSON `json:"types"`
}

// storageOp is one step of a load case. Op is "schema" (Text parsed in namespace Ns, each predicate and type
// written at Ts), "delete" and "delete_type" (State().Delete and DeleteType of Name at Ts), "empty_schema" and
// "empty_type" (an empty value under Name's key at Ts), "ban" (namespace Ns), or "malformed" (an unparsable key
// under the schema prefix at Ts).
type storageOp struct {
	Op   string `json:"op"`
	Text string `json:"text"`
	Name string `json:"name"`
	Ns   uint64 `json:"ns"`
	Ts   uint64 `json:"ts"`
}

// storageLoadJSON is Load of one predicate into an empty state: the error text, or what the state holds for it.
type storageLoadJSON struct {
	Predicate string            `json:"predicate"`
	Err       string            `json:"err"`
	Found     bool              `json:"found"`
	Schema    *schemaUpdateJSON `json:"schema"`
}

type storageLoadCase struct {
	Name string      `json:"name"`
	Ops  []storageOp `json:"ops"`
	// The state LoadFromDb loads, predicates and types sorted by name.
	Preds []schemaUpdateJSON `json:"preds"`
	Types []schemaTypeJSON   `json:"types"`
	Loads []storageLoadJSON  `json:"loads"`
}

// The malformed key under the schema prefix: a namespace and one byte, too short for an attribute's length.
var storageMalformedKey = []byte{x.ByteSchema, 0, 0, 0, 0, 0, 0, 0, 0, 7}

var storageLoadCases = []struct {
	name  string
	ops   []storageOp
	loads []string
}{
	{"empty store", nil, []string{"0-name", ""}},
	{"one predicate", []storageOp{{Op: "schema", Text: "name: string @index(exact) .", Ts: 2}},
		[]string{"0-name", "0-age"}},
	{"predicates and types of two namespaces", []storageOp{
		{Op: "schema", Text: "name: string @index(exact, term) @upsert @lang .\nfriend: [uid] @reverse @count .\n" +
			"type Person {\n name\n friend\n}", Ts: 2},
		{Op: "schema", Text: "age: int @index(int) .\ntype Animal {\n age\n}", Ns: 5, Ts: 3},
	}, []string{"0-name", "0-friend", "5-age", "5-name"}},
	{"a newer version replaces an older", []storageOp{
		{Op: "schema", Text: "name: string .", Ts: 2},
		{Op: "schema", Text: "name: string @index(exact) @count .", Ts: 5},
	}, []string{"0-name"}},
	{"a deleted predicate", []storageOp{
		{Op: "schema", Text: "name: string @index(exact) .\nage: int .", Ts: 2},
		{Op: "delete", Name: "0-name", Ts: 3},
	}, []string{"0-name", "0-age"}},
	{"a deleted type", []storageOp{
		{Op: "schema", Text: "name: string .\ntype Person {\n name\n}\ntype Pet {\n name\n}", Ts: 2},
		{Op: "delete_type", Name: "0-Person", Ts: 3},
	}, []string{"0-name"}},
	{"a predicate redefined after its deletion", []storageOp{
		{Op: "schema", Text: "name: string .", Ts: 2},
		{Op: "delete", Name: "0-name", Ts: 3},
		{Op: "schema", Text: "name: int .", Ts: 4},
	}, []string{"0-name"}},
	{"empty values", []storageOp{
		{Op: "empty_schema", Name: "0-name", Ts: 2},
		{Op: "empty_type", Name: "0-Person", Ts: 2},
	}, []string{"0-name"}},
	{"a banned namespace", []storageOp{
		{Op: "schema", Text: "name: string .\ntype Person {\n name\n}", Ts: 2},
		{Op: "schema", Text: "age: int .\ntype Animal {\n age\n}", Ns: 5, Ts: 3},
		{Op: "ban", Ns: 5},
	}, []string{"0-name", "5-age"}},
	{"a malformed key", []storageOp{
		{Op: "schema", Text: "name: string .", Ts: 2},
		{Op: "malformed", Ts: 3},
	}, []string{"0-name"}},
}

// storageWrite writes one version as updateSchema and updateType do.
func storageWrite(db *badger.DB, key, value []byte, ts uint64) error {
	txn := db.NewTransactionAt(ts, true)
	defer txn.Discard()
	e := &badger.Entry{Key: key, Value: value, UserMeta: posting.BitSchemaPosting}
	if err := txn.SetEntry(e.WithDiscard()); err != nil {
		return err
	}
	return txn.CommitAt(ts, nil)
}

func storageOpen() (*badger.DB, error) {
	opt := badger.DefaultOptions("").WithInMemory(true).WithLogger(nil).WithNamespaceOffset(x.NamespaceOffset)
	return badger.OpenManaged(opt)
}

// storageRead returns every version under the store, newest first per key.
func storageRead(db *badger.DB) ([]storageEntryJSON, error) {
	txn := db.NewTransactionAt(math.MaxUint64, false)
	defer txn.Discard()
	opt := badger.DefaultIteratorOptions
	opt.AllVersions = true
	itr := txn.NewIterator(opt)
	defer itr.Close()
	var out []storageEntryJSON
	for itr.Rewind(); itr.Valid(); itr.Next() {
		item := itr.Item()
		val, err := item.ValueCopy(nil)
		if err != nil {
			return nil, err
		}
		out = append(out, storageEntryJSON{
			Key:      hex.EncodeToString(item.KeyCopy(nil)),
			Value:    hex.EncodeToString(val),
			UserMeta: item.UserMeta(),
			Discard:  item.DiscardEarlierVersions(),
		})
	}
	return out, nil
}

// storageEntries writes each update into a store of its own and reads it back.
func storageEntries(preds []*pb.SchemaUpdate, types []*pb.TypeUpdate) ([]storageEntryJSON, []storageEntryJSON, error) {
	one := func(key []byte, m proto.Message) (storageEntryJSON, error) {
		db, err := storageOpen()
		if err != nil {
			return storageEntryJSON{}, err
		}
		defer db.Close()
		data, err := proto.Marshal(m)
		if err != nil {
			return storageEntryJSON{}, err
		}
		if err := storageWrite(db, key, data, 1); err != nil {
			return storageEntryJSON{}, err
		}
		read, err := storageRead(db)
		if err != nil {
			return storageEntryJSON{}, err
		}
		if len(read) != 1 {
			return storageEntryJSON{}, fmt.Errorf("want one stored version, got %d", len(read))
		}
		return read[0], nil
	}
	ps, ts := []storageEntryJSON{}, []storageEntryJSON{}
	for _, p := range preds {
		e, err := one(x.SchemaKey(p.Predicate), p)
		if err != nil {
			return nil, nil, err
		}
		ps = append(ps, e)
	}
	for _, t := range types {
		e, err := one(x.TypeKey(t.TypeName), t)
		if err != nil {
			return nil, nil, err
		}
		ts = append(ts, e)
	}
	return ps, ts, nil
}

func storageApply(db *badger.DB, op storageOp) error {
	switch op.Op {
	case "schema":
		parsed, err := schema.ParseWithNamespace(op.Text, op.Ns)
		if err != nil {
			return err
		}
		for _, p := range parsed.Preds {
			data, err := proto.Marshal(p)
			if err != nil {
				return err
			}
			if err := storageWrite(db, x.SchemaKey(p.Predicate), data, op.Ts); err != nil {
				return err
			}
		}
		for _, t := range parsed.Types {
			data, err := proto.Marshal(t)
			if err != nil {
				return err
			}
			if err := storageWrite(db, x.TypeKey(t.TypeName), data, op.Ts); err != nil {
				return err
			}
		}
		return nil
	case "delete":
		return schema.State().Delete(op.Name, op.Ts)
	case "delete_type":
		return schema.State().DeleteType(op.Name, op.Ts)
	case "empty_schema":
		return storageWrite(db, x.SchemaKey(op.Name), nil, op.Ts)
	case "empty_type":
		return storageWrite(db, x.TypeKey(op.Name), nil, op.Ts)
	case "ban":
		return db.BanNamespace(op.Ns)
	case "malformed":
		return storageWrite(db, storageMalformedKey, []byte{1}, op.Ts)
	}
	return fmt.Errorf("unknown storage op %q", op.Op)
}

func storageLoad(name string, ops []storageOp, loads []string) (storageLoadCase, error) {
	db, err := storageOpen()
	if err != nil {
		return storageLoadCase{}, err
	}
	defer db.Close()
	schema.Init(db)
	for _, op := range ops {
		if err := storageApply(db, op); err != nil {
			return storageLoadCase{}, err
		}
	}
	if err := schema.LoadFromDb(context.Background()); err != nil {
		return storageLoadCase{}, err
	}
	c := storageLoadCase{Name: name, Ops: ops, Preds: []schemaUpdateJSON{}, Types: []schemaTypeJSON{}, Loads: []storageLoadJSON{}}
	if c.Ops == nil {
		c.Ops = []storageOp{}
	}
	preds := schema.State().Predicates()
	sort.Strings(preds)
	for _, p := range preds {
		s, _ := schema.State().Get(context.Background(), p)
		c.Preds = append(c.Preds, schemaUpdateToJSON(&s))
	}
	typeNames := schema.State().Types()
	sort.Strings(typeNames)
	var types []*pb.TypeUpdate
	for _, t := range typeNames {
		typ, _ := schema.State().GetType(t)
		types = append(types, &typ)
	}
	c.Types = schemaTypesToJSON(types)
	for _, p := range loads {
		schema.State().DeleteAll()
		l := storageLoadJSON{Predicate: p}
		if err := schema.Load(p); err != nil {
			l.Err = err.Error()
		}
		if s, ok := schema.State().Get(context.Background(), p); ok {
			j := schemaUpdateToJSON(&s)
			l.Found, l.Schema = true, &j
		}
		c.Loads = append(c.Loads, l)
	}
	return c, nil
}

func genSchemaStorageEntries() (any, error) {
	corpus, err := schemaCorpus()
	if err != nil {
		return nil, err
	}
	var out []storageEntriesCase
	for _, s := range corpus {
		parsed, err := schema.Parse(s)
		if err != nil {
			continue
		}
		preds, types, err := storageEntries(parsed.Preds, parsed.Types)
		if err != nil {
			return nil, err
		}
		out = append(out, storageEntriesCase{Input: s, Initial: -1, Preds: preds, Types: types})
	}
	for _, ns := range []uint64{0, 5} {
		preds, types, err := storageEntries(schema.CompleteInitialSchema(ns), schema.CompleteInitialTypes(ns))
		if err != nil {
			return nil, err
		}
		out = append(out, storageEntriesCase{Initial: int64(ns), Preds: preds, Types: types})
	}
	return out, nil
}

func genSchemaStorageLoads() (any, error) {
	// LoadFromDb logs the malformed key; glog writes to stderr rather than to files in the temporary directory.
	if err := flag.Set("logtostderr", "true"); err != nil {
		return nil, err
	}
	var out []storageLoadCase
	for _, lc := range storageLoadCases {
		c, err := storageLoad(lc.name, lc.ops, lc.loads)
		if err != nil {
			return nil, fmt.Errorf("%s: %w", lc.name, err)
		}
		out = append(out, c)
	}
	return out, nil
}
