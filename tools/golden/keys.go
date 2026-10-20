// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_keys: Dgraph's key encodings (x/keys.go), their parse, namespaced attribute
// names and the reserved and pre-defined name checks.

import (
	"encoding/hex"
	"math"
	"strings"

	"github.com/dgraph-io/dgraph/v25/x"
)

func init() {
	register("keys", "ultragraphdb_core_keys", genKeys)
	register("keys_parse_errors", "ultragraphdb_core_keys", genKeysParseErrors)
	register("keys_namespace_attr", "ultragraphdb_core_keys", genKeysNamespaceAttr)
	register("keys_reserved", "ultragraphdb_core_keys", genKeysReserved)
}

var (
	goldenNamespaces = []uint64{0, 1, 133, 1 << 63}
	goldenAttrs      = []string{"name", "first-name", "名前", "", "~friend", "dgraph.type", strings.Repeat("a", 300)}
	goldenUids       = []uint64{1, 1 << 40, math.MaxUint64}
	goldenTerms      = []string{"", "a", "\x00\xff", "日本", "\x02\x00\x00\x00\x00\x00\x00\x00\x01"}
	goldenCounts     = []uint32{0, 1, math.MaxUint32}
	goldenStartUids  = []uint64{1000, math.MaxUint64}
)

// parsedCase is what x.Parse returns for a key, with the ParsedKey's predicates and derived prefixes.
type parsedCase struct {
	Attr           string `json:"attr"`
	Uid            uint64 `json:"uid,omitempty"`
	StartUid       uint64 `json:"start_uid,omitempty"`
	Term           string `json:"term,omitempty"`
	Count          uint32 `json:"count,omitempty"`
	ByteType       byte   `json:"byte_type,omitempty"`
	HasStartUid    bool   `json:"has_start_uid,omitempty"`
	IsData         bool   `json:"is_data,omitempty"`
	IsReverse      bool   `json:"is_reverse,omitempty"`
	IsCount        bool   `json:"is_count,omitempty"`
	IsCountRev     bool   `json:"is_count_rev,omitempty"`
	IsIndex        bool   `json:"is_index,omitempty"`
	IsSchema       bool   `json:"is_schema,omitempty"`
	IsType         bool   `json:"is_type,omitempty"`
	String         string `json:"string"`
	SkipPredicate  string `json:"skip_predicate,omitempty"`
	DataPrefix     string `json:"data_prefix,omitempty"`
	IndexPrefix    string `json:"index_prefix,omitempty"`
	ReversePrefix  string `json:"reverse_prefix,omitempty"`
	CountPrefix    string `json:"count_prefix,omitempty"`
	CountRevPrefix string `json:"count_rev_prefix,omitempty"`
}

// parseResult is the outcome of x.Parse: the parsed key, its error text, or a panic.
type parseResult struct {
	Parsed *parsedCase `json:"parsed"`
	Error  string      `json:"error,omitempty"`
	Panics bool        `json:"panics,omitempty"`
}

type keyCase struct {
	Kind      string `json:"kind"`
	Namespace uint64 `json:"namespace"`
	Attr      string `json:"attr"`
	Uid       uint64 `json:"uid,omitempty"`
	Term      string `json:"term,omitempty"`
	Count     uint32 `json:"count,omitempty"`
	StartUid  uint64 `json:"start_uid,omitempty"`
	Key       string `json:"key"`
	parseResult
}

// parseKey parses key; withPrefixes adds the ParsedKey's derived prefixes, which depend on its prefix and attribute alone.
func parseKey(key []byte, withPrefixes bool) (res parseResult) {
	defer func() {
		if r := recover(); r != nil {
			res = parseResult{Panics: true}
		}
	}()
	pk, err := x.Parse(key)
	if err != nil {
		return parseResult{Error: err.Error()}
	}
	p := &parsedCase{
		Attr: pk.Attr, Uid: pk.Uid, StartUid: pk.StartUid, Term: hex.EncodeToString([]byte(pk.Term)),
		Count: pk.Count, ByteType: pk.ByteType, HasStartUid: pk.HasStartUid,
		IsData: pk.IsData(), IsReverse: pk.IsReverse(), IsCount: pk.IsCount(), IsCountRev: pk.IsCountRev(),
		IsIndex: pk.IsIndex(), IsSchema: pk.IsSchema(), IsType: pk.IsType(), String: pk.String(),
	}
	if withPrefixes && key[0] != x.ByteUnused {
		p.SkipPredicate = hex.EncodeToString(pk.SkipPredicate())
		p.DataPrefix = hex.EncodeToString(pk.DataPrefix())
		p.IndexPrefix = hex.EncodeToString(pk.IndexPrefix())
		p.ReversePrefix = hex.EncodeToString(pk.ReversePrefix())
		p.CountPrefix = hex.EncodeToString(pk.CountPrefix(false))
		p.CountRevPrefix = hex.EncodeToString(pk.CountPrefix(true))
	}
	return parseResult{Parsed: p}
}

func genKeys() (any, error) {
	var cases []keyCase
	add := func(c keyCase, key []byte) {
		c.Key = hex.EncodeToString(key)
		c.parseResult = parseKey(key, c.Uid <= 1 && c.Count == 0 && c.Term == "" && c.StartUid != math.MaxUint64)
		cases = append(cases, c)
	}
	addWithSplits := func(c keyCase, key []byte) error {
		add(c, key)
		for _, start := range goldenStartUids {
			split, err := x.SplitKey(key, start)
			if err != nil {
				return err
			}
			s := c
			s.Kind = "split_" + c.Kind
			s.StartUid = start
			add(s, split)
		}
		return nil
	}
	for _, ns := range goldenNamespaces {
		for _, attr := range goldenAttrs {
			if len(attr) > 255 && ns != 0 && ns != 1<<63 {
				// The long attribute's two-byte length is covered by two namespaces; more would only add size.
				continue
			}
			nsAttr := x.NamespaceAttr(ns, attr)
			base := keyCase{Namespace: ns, Attr: attr}
			for _, uid := range goldenUids {
				c := base
				c.Uid = uid
				c.Kind = "data"
				if err := addWithSplits(c, x.DataKey(nsAttr, uid)); err != nil {
					return nil, err
				}
				c.Kind = "reverse"
				if err := addWithSplits(c, x.ReverseKey(nsAttr, uid)); err != nil {
					return nil, err
				}
			}
			for _, term := range goldenTerms {
				c := base
				c.Kind = "index"
				c.Term = hex.EncodeToString([]byte(term))
				if err := addWithSplits(c, x.IndexKey(nsAttr, term)); err != nil {
					return nil, err
				}
			}
			for _, count := range goldenCounts {
				c := base
				c.Count = count
				c.Kind = "count"
				if err := addWithSplits(c, x.CountKey(nsAttr, count, false)); err != nil {
					return nil, err
				}
				c.Kind = "count_rev"
				if err := addWithSplits(c, x.CountKey(nsAttr, count, true)); err != nil {
					return nil, err
				}
			}
			c := base
			c.Kind = "schema"
			add(c, x.SchemaKey(nsAttr))
			c.Kind = "type"
			add(c, x.TypeKey(nsAttr))
			c.Kind = "index_after_all_terms"
			add(c, x.IndexKeyAfterAllTerms(nsAttr))
			c.Kind = "predicate_prefix"
			add(c, x.PredicatePrefix(nsAttr))
		}
		add(keyCase{Kind: "data_prefix", Namespace: ns}, x.DataPrefix(ns))
	}
	add(keyCase{Kind: "schema_prefix"}, x.SchemaPrefix())
	add(keyCase{Kind: "type_prefix"}, x.TypePrefix())
	return cases, nil
}

type parseErrorCase struct {
	Name string `json:"name"`
	Key  string `json:"key"`
	parseResult
}

type splitErrorCase struct {
	Name     string `json:"name"`
	Key      string `json:"key"`
	StartUid uint64 `json:"start_uid"`
	Split    string `json:"split"`
	Error    string `json:"error"`
}

type parseErrorFile struct {
	Parse []parseErrorCase `json:"parse"`
	Split []splitErrorCase `json:"split"`
}

func genKeysParseErrors() (any, error) {
	root := x.AttrInRootNamespace
	split := func(key []byte, start uint64) []byte {
		s, err := x.SplitKey(key, start)
		x.Check(err)
		return s
	}
	appendByte := func(key []byte, b byte) []byte { return append(append([]byte{}, key...), b) }
	withType := func(key []byte, typ byte) []byte {
		k := append([]byte{}, key...)
		k[len(k)-9] = typ
		return k
	}
	inputs := []struct {
		name string
		key  []byte
	}{
		{"empty", []byte{}},
		{"one byte", []byte{1}},
		{"two bytes", []byte{1, 2}},
		{"short attribute length", []byte{1, 0x00, 0x04, 1, 2}},
		{"nine bytes", make([]byte, 9)},
		{"ten bytes", make([]byte, 10)},
		{"eleven bytes", make([]byte, 11)},
		{"attribute longer than key", append(make([]byte, 9), 0x00, 0x05, 'a', 'b')},
		{"unused prefix", append([]byte{0xff}, make([]byte, 8)...)},
		{"unused prefix with rest", append([]byte{0xff}, make([]byte, 20)...)},
		{"data uid zero", x.DataKey(root("bad uid"), 0)},
		{"reverse uid zero", x.ReverseKey(root("bad uid"), 0)},
		{"data uid zero unicode", x.DataKey(x.NamespaceAttr(7, "名前\t\"x\"\\"), 0)},
		{"data uid short", x.DataKey(root("name"), 1)[:len(x.DataKey(root("name"), 1))-1]},
		{"data uid short escapes", x.DataKey(root("名\t\"x\"\\\x7f\u00a0\u200b\U0001F600"), 1)[:len(x.DataKey(root("名\t\"x\"\\\x7f\u00a0\u200b\U0001F600"), 1))-1]},
		{"data uid short unicode", x.DataKey(root("名前"), 1)[:len(x.DataKey(root("名前"), 1))-1]},
		{"data with trailing byte", appendByte(x.DataKey(root("name"), 1), 7)},
		{"split data with trailing byte", appendByte(split(x.DataKey(root("aa"), 1), 10), 0)},
		{"split reverse with trailing byte", appendByte(split(x.ReverseKey(root("aa"), 1), 10), 0)},
		{"split count with trailing byte", appendByte(split(x.CountKey(root("aa"), 0, false), 10), 0)},
		{"split count rev with trailing byte", appendByte(split(x.CountKey(root("aa"), 0, true), 10), 0)},
		{"split data short start uid", split(x.DataKey(root("aa"), 1), 10)[:len(split(x.DataKey(root("aa"), 1), 10))-1]},
		{"split index short", append([]byte{x.ByteSplit}, x.IndexKey(root("aa"), "abc")[1:]...)},
		{"split index exactly eight", split(x.IndexKey(root("aa"), ""), 5)},
		{"count short", x.CountKey(root("aa"), 5, false)[:len(x.CountKey(root("aa"), 5, false))-1]},
		{"count rev with trailing bytes", append(x.CountKey(root("aa"), 5, true), 1, 2, 3)},
		{"invalid data type", withType(x.DataKey(root("aa"), 1), 0x05)},
		{"invalid data type 0x01", withType(x.DataKey(root("aa"), 1), 0x01)},
		{"schema key with trailing bytes", append(x.SchemaKey(root("aa")), 1, 2)},
		{"type key with trailing bytes", append(x.TypeKey(root("aa")), 0xff)},
		{"predicate prefix", x.PredicatePrefix(root("aa"))},
		{"unknown prefix data", append([]byte{0x07}, x.DataKey(root("aa"), 3)[1:]...)},
		{"split data with uid zero", split(x.DataKey(root("aa"), 0), 10)},
	}
	var out parseErrorFile
	for _, in := range inputs {
		out.Parse = append(out.Parse, parseErrorCase{Name: in.name, Key: hex.EncodeToString(in.key),
			parseResult: parseKey(in.key, true)})
	}
	splits := []struct {
		name  string
		key   []byte
		start uint64
	}{
		{"schema key", x.SchemaKey(root("aa")), 1},
		{"type key", x.TypeKey(root("aa")), 1},
		{"split key", split(x.DataKey(root("aa"), 1), 3), 1},
		{"empty key", []byte{}, 7},
		{"data key", x.DataKey(root("aa"), 1), 0},
	}
	for _, s := range splits {
		c := splitErrorCase{Name: s.name, Key: hex.EncodeToString(s.key), StartUid: s.start}
		res, err := x.SplitKey(s.key, s.start)
		if err != nil {
			c.Error = err.Error()
		} else {
			c.Split = hex.EncodeToString(res)
		}
		out.Split = append(out.Split, c)
	}
	return out, nil
}

type namespaceAttrCase struct {
	Namespace     uint64 `json:"namespace"`
	Attr          string `json:"attr"`
	Formatted     string `json:"formatted"`
	ParsedNs      uint64 `json:"parsed_namespace"`
	ParsedAttr    string `json:"parsed_attr"`
	NsBytes       string `json:"namespace_bytes"`
	IsReverse     bool   `json:"is_reverse_attr"`
	ReversePanics bool   `json:"is_reverse_attr_panics"`
}

func genKeysNamespaceAttr() (any, error) {
	var cases []namespaceAttrCase
	attrs := append([]string{"a-b-c", "-", "~", "~a-b"}, goldenAttrs...)
	for _, ns := range append(goldenNamespaces, 15, 16, 255, 0xdeadbeef, math.MaxUint64) {
		for _, attr := range attrs {
			f := x.NamespaceAttr(ns, attr)
			pns, pattr := x.ParseNamespaceAttr(f)
			nsb, _ := x.ParseNamespaceBytes(f)
			c := namespaceAttrCase{Namespace: ns, Attr: attr, Formatted: f, ParsedNs: pns, ParsedAttr: pattr,
				NsBytes: hex.EncodeToString(nsb)}
			func() {
				defer func() {
					if recover() != nil {
						c.ReversePanics = true
					}
				}()
				c.IsReverse = x.IsReverseAttr(f)
			}()
			cases = append(cases, c)
		}
	}
	return cases, nil
}

type reservedCase struct {
	Name                    string `json:"name"`
	IsReservedPredicate     bool   `json:"is_reserved_predicate"`
	IsPreDefinedPredicate   bool   `json:"is_pre_defined_predicate"`
	IsAclPredicate          bool   `json:"is_acl_predicate"`
	IsOtherReservedPred     bool   `json:"is_other_reserved_predicate"`
	IsInternalPredicate     bool   `json:"is_internal_predicate"`
	IsReservedType          bool   `json:"is_reserved_type"`
	IsPreDefinedType        bool   `json:"is_pre_defined_type"`
	IsRegisteredReservedPrd bool   `json:"is_registered_reserved_predicate"`
}

func genKeysReserved() (any, error) {
	names := []string{
		"dgraph.type", "DGRAPH.TYPE", "Dgraph.Type", "dgraph.xid", "Dgraph.Password", "dgraph.password",
		"dgraph.user.group", "dgraph.rule.predicate", "dgraph.rule.permission", "dgraph.acl.rule",
		"dgraph.graphql.schema", "DGRAPH.GRAPHQL.SCHEMA", "dgraph.graphql.xid", "dgraph.graphql.p_query",
		"dgraph.drop.op", "dgraph.namespace.id", "dgraph.namespace.name", "dgraph.blah", "dgraph.", "dgraph",
		"DGRAPH.", "dgraphx", "uid", "UID", "Uid", "person.name", "dgraph.graphql", "dgraph.type.User",
		"dgraph.type.user", "dgraph.type.Group", "dgraph.type.Rule", "dgraph.graphql.persisted_query",
		"dgraph.namespace", "~dgraph.type", "", "İdgraph.x", "dgraph.Σ", "a-dgraph.type",
	}
	var cases []reservedCase
	for _, n := range names {
		nsAttr := x.AttrInRootNamespace(n)
		cases = append(cases, reservedCase{
			Name:                    n,
			IsReservedPredicate:     x.IsReservedPredicate(nsAttr),
			IsPreDefinedPredicate:   x.IsPreDefinedPredicate(nsAttr),
			IsAclPredicate:          x.IsAclPredicate(n),
			IsOtherReservedPred:     x.IsOtherReservedPredicate(n),
			IsInternalPredicate:     x.IsInternalPredicate(nsAttr),
			IsReservedType:          x.IsReservedType(nsAttr),
			IsPreDefinedType:        x.IsPreDefinedType(nsAttr),
			IsRegisteredReservedPrd: x.IsRegisteredReservedPredicate(nsAttr),
		})
	}
	return cases, nil
}
