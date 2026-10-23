// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// Golden vectors for ultragraphdb_core_tok's tokenizers (tok/tok.go, tokens.go, langbase.go, index_factory.go),
// through the fork's exported functions:
//
//   - tok_tokens: BuildTokens of every tokenizer over typed values and the text corpus (tok_corpus.go), with the
//     languages GetTokenizerForLang takes, and the ngram tokens and query tokens of GetNGramTokens and
//     GetNGramQueryTokens. Ngram and geo tokens come out of a Go map, so they are sorted; every other list is in
//     Dgraph's order. Where the port's tokens differ, the case holds them too: Dgraph's tokens of the text lowercased
//     by tokLowerWords (ledger 16, 30), and the ngram index tokens of GetNGramTokens with the language (ledger 14).
//   - tok_lang_base: LangBase over language tags.
//   - tok_registry: each registered tokenizer's name, type, identifier, sortable and lossy flags.
//   - tok_hnsw_options: the hnsw factory's parsed options, its errors and FactoryCreateSpec names.

import (
	"encoding/hex"
	"fmt"
	"math"
	"reflect"
	"sort"
	"strconv"

	"github.com/blevesearch/segment"

	"github.com/dgraph-io/dgraph/v25/protos/pb"
	"github.com/dgraph-io/dgraph/v25/tok"
	"github.com/dgraph-io/dgraph/v25/types"
)

func init() {
	register("tok_tokens", "ultragraphdb_core_tok", genTokTokens)
	register("tok_lang_base", "ultragraphdb_core_tok", genTokLangBase)
	register("tok_registry", "ultragraphdb_core_tok", genTokRegistry)
	register("tok_hnsw_options", "ultragraphdb_core_tok", genTokHnswOptions)
}

type tokTokensCase struct {
	Tokenizer string `json:"tokenizer"`
	// Lang is the language given to GetTokenizerForLang, or to GetNGramTokens and GetNGramQueryTokens.
	Lang string `json:"lang"`
	// Via is "build" (BuildTokens of GetTokenizerForLang), "ngram" (GetNGramTokens) or "ngram_query"
	// (GetNGramQueryTokens).
	Via string `json:"via"`
	// Kind is the value's type: string (Input in hex), int (decimal), float (IEEE bits in hex), bigfloat,
	// datetime and geo (the text Dgraph converts from).
	Kind   string   `json:"kind"`
	Input  string   `json:"input"`
	Tokens []string `json:"tokens"` // hex, identifier byte included
	Sorted bool     `json:"sorted"`
	Error  string   `json:"error"`
	// Fixed are the port's tokens where they differ from Tokens (hex, identifier byte included).
	Fixed []string `json:"fixed,omitempty"`
}

// tokSpaceSplit are the language bases whose term tokens split the text on spaces, unanalyzed.
var tokSpaceSplit = map[string]bool{"zh": true, "ja": true, "th": true, "lo": true, "my": true, "bo": true,
	"km": true, "kxm": true}

// tokLowerWords is s with each word segment lowercased by tokToLower. Dgraph's term, fulltext and ngram tokens of it
// are the port's tokens of s, as to_lower then changes no term. It panics where lowercasing changes the segments'
// types, where that would not hold.
func tokLowerWords(s string) string {
	segTypes := func(b []byte) ([]int, [][]byte) {
		var types []int
		var segs [][]byte
		seg := segment.NewWordSegmenterDirect(b)
		for seg.Segment() {
			types = append(types, seg.Type())
			segs = append(segs, seg.Bytes())
		}
		check(seg.Err())
		return types, segs
	}
	types, segs := segTypes([]byte(s))
	var out []byte
	for i, w := range segs {
		if types[i] != segment.None {
			w = tokToLower(w)
		}
		out = append(out, w...)
	}
	if lowered, _ := segTypes(out); !reflect.DeepEqual(lowered, types) {
		panic(fmt.Sprintf("lowercasing %q changes its segments", s))
	}
	return string(out)
}

// tokPortTokens are the port's tokens of the string s where they may differ from Dgraph's, nil where they cannot:
// the analyzed tokenizers over tokLowerWords(s), and ngram with its language for the index too.
func tokPortTokens(name, lang, via, s string) []string {
	if !(name == "fulltext" || name == "ngram" || (name == "term" && !tokSpaceSplit[tok.LangBase(lang)])) {
		return nil
	}
	lower := tokLowerWords(s)
	var toks []string
	var err error
	switch {
	case via == "ngram_query":
		toks, err = tok.GetNGramQueryTokens([]string{lower}, lang)
	case name == "ngram":
		toks, err = tok.GetNGramTokens([]string{lower}, lang)
	default:
		t, _ := tok.GetTokenizer(name)
		toks, err = tok.BuildTokens(lower, tok.GetTokenizerForLang(t, lang))
	}
	check(err)
	return toks
}

// tokTokenLangs are language tags reduced by LangBase: "" (none), every language with a stemmer, stop words or space
// splitting, and tags reducing to them or to English.
var tokTokenLangs = []string{
	"", "en", "ar", "bg", "ca", "ckb", "cs", "da", "de", "el", "es", "eu", "fa", "fi", "fr", "ga", "gl", "hi",
	"hu", "hy", "id", "it", "nl", "no", "pt", "ro", "ru", "sv", "tr", "zh", "ja", "ko", "th", "lo", "my", "bo",
	"km", "kxm", "zh-Hant", "en-US", "sh", "nb", "und", "x-klingon", "xxx_such_language",
}

// tokMixedLangs are the languages the mixed corpus is tokenized under.
var tokMixedLangs = []string{"", "en", "de", "fr", "ru", "ar", "hi", "tr", "el", "zh", "ja", "ko", "th", "zh-Hant",
	"xxx_such_language"}

func genTokTokens() (any, error) {
	var out []tokTokensCase
	hexes := func(toks []string, sorted bool) []string {
		if sorted {
			sort.Strings(toks)
		}
		out := []string{}
		for _, t := range toks {
			out = append(out, hex.EncodeToString([]byte(t)))
		}
		return out
	}
	// fixed are the port's tokens, nil where they equal Dgraph's.
	add := func(name, lang, via, kind, input string, toks []string, err error, sorted bool, fixed []string) {
		c := tokTokensCase{Tokenizer: name, Lang: lang, Via: via, Kind: kind, Input: input, Sorted: sorted,
			Tokens: []string{}}
		if err != nil {
			c.Error = err.Error()
		} else {
			c.Tokens = hexes(toks, sorted)
			if fixed != nil {
				if f := hexes(fixed, sorted); !reflect.DeepEqual(f, c.Tokens) {
					c.Fixed = f
				}
			}
		}
		out = append(out, c)
	}
	build := func(name, lang, kind, input string, v any) {
		t, ok := tok.GetTokenizer(name)
		if !ok {
			panic("no tokenizer " + name)
		}
		toks, err := tok.BuildTokens(v, tok.GetTokenizerForLang(t, lang))
		var fixed []string
		if s, ok := v.(string); ok {
			fixed = tokPortTokens(name, lang, "build", s)
		}
		add(name, lang, "build", kind, input, toks, err, name == "ngram" || name == "geo", fixed)
	}

	// Sortable and typed tokenizers.
	ints := []int64{math.MinInt64, math.MinInt64 + 1, -1 << 32, -1543, -3, -2, -1, 0, 1, 2, 3, 4, 123, 234, 10000,
		1<<24 + 10, -1<<24 - 1, 1 << 32, math.MaxInt64 - 1, math.MaxInt64}
	for _, i := range ints {
		build("int", "", "int", strconv.FormatInt(i, 10), i)
	}
	floats := []float64{0, math.Copysign(0, -1), -2.7, 2.7, -0.5, 0.5, 1.5, -1.5, 1e18, -1e18, 9.2e18, -9.2e18,
		1e20, -1e20, 9.223372036854775807e18, math.Inf(1), math.Inf(-1), math.NaN(), math.SmallestNonzeroFloat64,
		math.MaxFloat64, -math.MaxFloat64, 123456.789, -987654.321}
	for _, f := range floats {
		build("float", "", "float", strconv.FormatUint(math.Float64bits(f), 16), f)
	}
	bigs := []string{"0", "-0", "2.7", "-2.7", "123456789012345678901234567890", "-123456789012345678901234567890",
		"9223372036854775807", "9223372036854775808", "-9223372036854775808", "-9223372036854775809", "1e-300",
		"1e300", "-1e300", "Inf", "-Inf", "+Inf", "0.999999999999999999999", "-0.999999999999999999999", "42",
		"1.5e10", "0x1p62", "0x1p63", "-0x1p63"}
	for _, s := range bigs {
		v, err := types.Convert(types.Val{Tid: types.StringID, Value: []byte(s)}, types.BigFloatID)
		if err != nil {
			add("bigfloat", "", "build", "bigfloat", s, nil, err, false, nil)
			continue
		}
		build("bigfloat", "", "bigfloat", s, v.Value)
	}
	for _, b := range []bool{false, true} {
		build("bool", "", "bool", strconv.FormatBool(b), b)
	}
	times := []string{"2017-01-01T12:12:12Z", "2017-01-01T12:12:12+01:00", "2017-01-01T00:30:00+05:30",
		"2016-12-31T23:30:00-01:00", "1999-12-31T23:59:59.999999999Z", "2000-02-29T12:00:00Z", "0001-01-01T00:00:00Z",
		"9999-12-31T23:59:59Z", "1970-01-01T00:00:00Z", "1969-07-20T20:17:40Z", "2018", "2018-05", "2018-05-30",
		"2018-05-30T09:30:10", "2018-05-30T09:30:10.5", "2018-05-30T09:30:10.123456789-07:00",
		"2018-05-30T09:30:10+14:00", "2018-05-30T09:30:10-12:00"}
	for _, s := range times {
		v, err := types.Convert(types.Val{Tid: types.StringID, Value: []byte(s)}, types.DateTimeID)
		if err != nil {
			add("year", "", "build", "datetime", s, nil, err, false, nil)
			continue
		}
		for _, name := range []string{"year", "month", "day", "hour"} {
			build(name, "", "datetime", s, v.Value)
		}
	}
	geos := []string{
		`{'type':'Point','coordinates':[-122.082506,37.4249518]}`,
		`{'type':'Point','coordinates':[1,2]}`,
		`{'type':'Point','coordinates':[0,0]}`,
		`{'type':'Point','coordinates':[180,90]}`,
		`{'type':'Point','coordinates':[-180,-90]}`,
		`{'type':'Point','coordinates':[13.404954,52.520008]}`,
		`{'type':'Polygon','coordinates':[[[-122,37],[-123,37],[-123,38],[-122,38],[-122,37]]]}`,
		`{'type':'Polygon','coordinates':[[[0,0],[10,0],[10,10],[0,10],[0,0]],[[2,2],[2,4],[4,4],[2,2]]]}`,
		`{'type':'Polygon','coordinates':[[[13.3,52.4],[13.5,52.4],[13.5,52.6],[13.3,52.6],[13.3,52.4]]]}`,
		`{'type':'MultiPolygon','coordinates':[[[[-112,39],[-113,39],[-113,40],[-112,40],[-112,39]]],` +
			`[[[71.09,42.35],[72.09,42.35],[72.09,41.35],[71.09,41.35],[71.09,42.35]]]]}`,
		`{'type':'LineString','coordinates':[[0,0],[1,1]]}`,
		`{'type':'MultiPoint','coordinates':[[1,2],[3,4]]}`,
		`{'type':'MultiLineString','coordinates':[[[1,2],[3,4]]]}`,
		`{'type':'GeometryCollection','geometries':[{'type':'Point','coordinates':[1,2]}]}`,
		`{'type':'Point','coordinates':[1,2,3]}`,
	}
	for _, s := range geos {
		v, err := types.Convert(types.Val{Tid: types.StringID, Value: []byte(s)}, types.GeoID)
		if err != nil {
			add("geo", "", "build", "geo", s, nil, err, true, nil)
			continue
		}
		build("geo", "", "geo", s, v.Value)
	}
	build("hnsw", "", "vfloat", "", []float32{1, 2, 3})

	// String tokenizers over the corpus: each language's sentences under that language, no language and English;
	// every other string under the languages of tokMixedLangs.
	type item struct {
		s     string
		langs []string
	}
	var items []item
	for _, lang := range tokSortedLangs() {
		for _, s := range tokLangSentences[lang] {
			items = append(items, item{s, []string{"", "en", lang}})
		}
	}
	mixed := append(append([]string{}, tokMixedCorpus...),
		"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
		"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef and more", "ab", "abc")
	for _, s := range mixed {
		items = append(items, item{s, tokMixedLangs})
	}
	for _, s := range tokRandomStrings(20261020, 400) {
		items = append(items, item{s, []string{"", "zh", "de"}})
	}
	for _, it := range items {
		s := it.s
		in := hex.EncodeToString([]byte(s))
		for _, name := range []string{"exact", "trigram", "hash", "sha256", "ngram"} {
			build(name, "", "string", in, s)
		}
		for _, lang := range it.langs {
			for _, name := range []string{"term", "fulltext"} {
				build(name, lang, "string", in, s)
			}
			toks, err := tok.GetNGramTokens([]string{s}, lang)
			add("ngram", lang, "ngram", "string", in, toks, err, true, tokPortTokens("ngram", lang, "ngram", s))
			toks, err = tok.GetNGramQueryTokens([]string{s}, lang)
			add("ngram", lang, "ngram_query", "string", in, toks, err, true,
				tokPortTokens("ngram", lang, "ngram_query", s))
		}
	}
	// GetTokenizerForLang keeps the ngram tokenizer as it is, whatever the language; the port gives it the language
	// (ledger 14).
	for _, lang := range []string{"de", "es", "zh"} {
		for _, s := range tokLangSentences[lang] {
			build("ngram", lang, "string", hex.EncodeToString([]byte(s)), s)
		}
	}
	// Exact tokens with a language: German strings, the language sentences and the mixed corpus under many tags.
	exactStrings := []string{"Äpfel", "Apfel", "apfel", "Zebra", "zebra", "Öl", "Ofen", "Straße", "Strasse",
		"Müller", "Mueller", "Muller", "ß", "", "a b", "Ärger", "Arzt", "über", "Ufer"}
	for _, lang := range tokSortedLangs() {
		exactStrings = append(exactStrings, tokLangSentences[lang]...)
	}
	exactStrings = append(exactStrings, tokMixedCorpus...)
	for _, lang := range []string{"de", "en", "fr", "sv", "da", "es", "tr", "ja", "zh", "ko", "ru", "ar", "hi",
		"und", "x!", "xxx_such_language", "de-u-co-phonebk", "sv-u-co-reformed", "en-US", "zh-Hant"} {
		for _, s := range exactStrings {
			build("exact", lang, "string", hex.EncodeToString([]byte(s)), s)
		}
	}
	// Values of a type a string tokenizer does not take.
	for _, name := range []string{"term", "fulltext", "ngram", "exact", "trigram", "hash", "sha256"} {
		build(name, "", "int", "123", int64(123))
	}
	return out, nil
}

type tokLangBaseCase struct {
	In  string `json:"in"`
	Out string `json:"out"`
}

func genTokLangBase() (any, error) {
	tags := []string{"zh", "zh-hant", "zh-hans", "es-es", "ar-001", "ca-ES-valencia", "ca-ES-valencia-u-va-posix",
		"ca-ES-valencia-u-co-phonebk", "ca-ES-valencia-u-co-phonebk-va-posix", "x-klingon", "en-US",
		"en-US-u-va-posix", "en", "en-u-co-phonebk", "en-001", "sh", "en-GB-u-rg-uszzzz", "en-GB-u-rg-uszzzz-va-posix",
		"en-GB-u-co-phonebk-rg-uszzzz", "en-GB-u-co-phonebk-rg-uszz", "", "no_such_language", "xxx_such_language",
		"zh-Hant", "und", "x!", "nb", "no", "kxm", "ckb", "iw", "in", "ji", "mo", "tl", "fil", "und-Latn", "und-CN",
		"und-Cyrl-RU", "und-Arab", "de-CH-1996", "sr-Latn", "ZH", "EN-us", "en_US", "a", "abcdefghi", "qaa"}
	tags = append(tags, tokTokenLangs...)
	var out []tokLangBaseCase
	for _, t := range tags {
		out = append(out, tokLangBaseCase{In: t, Out: tok.LangBase(t)})
	}
	return out, nil
}

type tokRegistryCase struct {
	Name       string `json:"name"`
	Found      bool   `json:"found"`
	Type       string `json:"type"`
	Identifier int    `json:"identifier"`
	Sortable   bool   `json:"sortable"`
	Lossy      bool   `json:"lossy"`
	Factory    bool   `json:"factory"`
	// LangIdentifier is the identifier of GetTokenizerForLang(t, "de").
	LangIdentifier int `json:"lang_identifier"`
}

func genTokRegistry() (any, error) {
	var out []tokRegistryCase
	for _, n := range []string{"bigfloat", "geo", "int", "float", "year", "hour", "month", "day", "exact", "bool",
		"trigram", "hash", "term", "fulltext", "ngram", "sha256", "hnsw", "exactlang", "vfloat", "HNSW", "Term", ""} {
		t, ok := tok.GetTokenizer(n)
		c := tokRegistryCase{Name: n, Found: ok}
		if ok {
			c.Type = t.Type()
			c.Identifier = int(t.Identifier())
			c.Sortable = t.IsSortable()
			c.Lossy = t.IsLossy()
			_, c.Factory = tok.GetIndexFactory(n)
			c.LangIdentifier = int(tok.GetTokenizerForLang(t, "de").Identifier())
		}
		out = append(out, c)
	}
	return out, nil
}

type tokHnswCase struct {
	// Options are key, value pairs given to GetFactoryCreateSpecFromSpec under Factory.
	Factory string     `json:"factory"`
	Options [][]string `json:"options"`
	Name    string     `json:"name"`  // FactoryCreateSpec.Name(), when it succeeds
	Error   string     `json:"error"` // otherwise
	// Parsed are GetParsedOption's results for each pair: the value as %v and its %T, or the error.
	Parsed []tokHnswParsed `json:"parsed"`
}

type tokHnswParsed struct {
	Value string `json:"value"`
	Type  string `json:"type"`
	Error string `json:"error"`
}

func genTokHnswOptions() (any, error) {
	sets := [][][]string{
		{},
		{{"metric", "euclidean"}},
		{{"metric", "cosine"}},
		{{"metric", "dotproduct"}},
		{{"metric", "manhattan"}},
		{{"metric", "Euclidean"}},
		{{"metric", ""}},
		{{"exponent", "4"}},
		{{"exponent", "4"}, {"metric", "cosine"}},
		{{"maxLevels", "5"}, {"efConstruction", "100"}, {"efSearch", "40"}},
		{{"efSearch", "40"}, {"maxLevels", "5"}, {"exponent", "6"}, {"efConstruction", "100"}, {"metric", "dotproduct"}},
		{{"exponent", "x"}},
		{{"exponent", "1.5"}},
		{{"exponent", "-3"}},
		{{"exponent", " 3"}},
		{{"exponent", "+3"}},
		{{"exponent", "99999999999999999999"}},
		{{"exponent", ""}},
		{{"maxlevels", "3"}},
		{{"unknown", "1"}},
		{{"metric", "cosine"}, {"metric", "euclidean"}},
		{{"efSearch", "0x10"}},
		{{"efSearch", "1_000"}},
	}
	var out []tokHnswCase
	for _, factory := range []string{"hnsw", "nosuch"} {
		for _, set := range sets {
			spec := &pb.VectorIndexSpec{Name: factory}
			for _, kv := range set {
				spec.Options = append(spec.Options, &pb.OptionPair{Key: kv[0], Value: kv[1]})
			}
			c := tokHnswCase{Factory: factory, Options: set, Parsed: []tokHnswParsed{}}
			if c.Options == nil {
				c.Options = [][]string{}
			}
			fcs, err := tok.GetFactoryCreateSpecFromSpec(spec)
			if err != nil {
				c.Error = err.Error()
			} else {
				c.Name = fcs.Name()
			}
			if f, ok := tok.GetIndexFactory(factory); ok {
				ao := f.AllowedOptions()
				for _, kv := range set {
					v, err := ao.GetParsedOption(kv[0], kv[1])
					p := tokHnswParsed{}
					if err != nil {
						p.Error = err.Error()
					} else {
						p.Value = fmt.Sprintf("%v", v)
						p.Type = fmt.Sprintf("%T", v)
						if kv[0] == "metric" {
							p.Value = kv[1] // a SimilarityType holds functions; its name is the option
						}
					}
					c.Parsed = append(c.Parsed, p)
				}
			}
			out = append(out, c)
		}
	}
	return out, nil
}
