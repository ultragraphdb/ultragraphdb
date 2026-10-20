// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

import (
	"bytes"
	"encoding/hex"
	"fmt"
	"math/rand"
	"sort"
	"strings"
	"unicode/utf8"

	"github.com/dgraph-io/dgraph/v25/tok"
	"golang.org/x/text/collate"
	"golang.org/x/text/language"
)

// Golden vectors for ultragraphdb_core_text, generated from golang.org/x/text v0.41.0 as Dgraph v25.5.0 calls it:
//
//   - text_lang_base: language.Parse, Tag.String, Tag.Base and Dgraph's tok.LangBase for language tags;
//   - text_collate_keys: collate.New(tag).KeyFromString for a corpus of strings, with the tag Dgraph's exact
//     tokenizer builds its collator from (language.Parse, or English when that fails);
//   - text_collate_compare: collate.New(tag).CompareString over every pair of a smaller corpus;
//   - text_lang_random and text_collate_random: the same for tags and strings drawn at random (a fixed seed) from
//     subtags and runes chosen to reach the parser's and the collator's rarer paths;
//   - text_collate_invalid and text_collate_invalid_compare: keys and comparisons of Go strings holding invalid
//     UTF-8, as Dgraph's string values may (texts and keys in hex, as encoding/json would replace invalid bytes).

func init() {
	register("text_lang_base", "ultragraphdb_core_text", textLangBase)
	register("text_collate_keys", "ultragraphdb_core_text", textCollateKeys)
	register("text_collate_compare", "ultragraphdb_core_text", textCollateCompare)
	register("text_lang_random", "ultragraphdb_core_text", textLangRandom)
	register("text_collate_random", "ultragraphdb_core_text", textCollateRandom)
	register("text_collate_invalid", "ultragraphdb_core_text", textCollateInvalid)
	register("text_collate_invalid_compare", "ultragraphdb_core_text", textCollateInvalidCompare)
}

type textLangBaseCase struct {
	Tag        string `json:"tag"`
	ParseError string `json:"parse_error"` // empty when language.Parse succeeds
	String     string `json:"string"`      // Tag.String of the parsed tag, when it parsed
	IsUnd      bool   `json:"is_und"`      // the parsed tag equals language.Und
	Base       string `json:"base"`
	Confidence string `json:"confidence"`
	LangBase   string `json:"lang_base"` // Dgraph's tok.LangBase
}

// textLanguageTags are the tags of the language-base vectors; every two-letter code is added to them.
var textLanguageTags = []string{
	// Dgraph's stemmer and stop-word languages.
	"ar", "bg", "ca", "ckb", "cs", "da", "de", "el", "en", "es", "eu", "fa", "fi", "fr", "ga", "gl", "hi", "hu",
	"hy", "id", "it", "ja", "ko", "nl", "no", "pt", "ro", "ru", "sv", "tr", "zh",
	// Dgraph's tests (tok/langbase_test.go and the query tests).
	"zh-hant", "zh-hans", "es-es", "ar-001", "ca-ES-valencia", "ca-ES-valencia-u-va-posix",
	"ca-ES-valencia-u-co-phonebk", "ca-ES-valencia-u-co-phonebk-va-posix", "x-klingon", "en-US",
	"en-US-u-va-posix", "en-u-co-phonebk", "en-001", "sh", "en-GB-u-rg-uszzzz", "en-GB-u-rg-uszzzz-va-posix",
	"en-GB-u-co-phonebk-rg-uszzzz", "en-GB-u-co-phonebk-rg-uszz", "", "no_such_language", "xxx_such_language",
	"jp", "cn", "xx", "pl",
	// The spec scenario.
	"zh-Hant", "und", "x!",
	// Regional variants and scripts.
	"de-DE", "de-AT", "de-CH", "de-LI", "en-GB", "en-AU", "en-IN", "en-CA", "fr-CA", "fr-BE", "fr-CH", "es-MX",
	"es-419", "es-AR", "pt-BR", "pt-PT", "zh-CN", "zh-TW", "zh-HK", "zh-SG", "zh-MO", "zh-Hans-CN", "zh-Hant-TW",
	"zh-Hant-HK", "sr-Latn", "sr-Cyrl", "sr-Latn-RS", "sr-ME", "bs-Cyrl", "bs-Latn", "uz-Arab", "uz-Cyrl",
	"az-Cyrl", "pa-Arab", "pa-PK", "mn-Mong", "ja-JP", "ko-KR", "ru-RU", "sv-FI", "nb-NO", "nn-NO", "se-NO",
	"und-US", "und-DE", "und-TW", "und-HK", "und-BR", "und-PT", "und-CN", "und-JP", "und-Latn", "und-Cyrl",
	"und-Hans", "und-Hant", "und-Arab", "und-Deva", "und-Jpan", "und-Kore", "und-Latn-DE", "und-Jpan-BR",
	"und-001", "und-150", "und-419", "und-ZZ", "und-Zzzz", "und-Qaai", "und-AQ", "und-XK", "und-QO", "und-EU",
	"und-UN", "und-CS", "und-YU", "und-SU", "und-DD", "und-BU", "und-TP", "und-ZR", "und-Aghb", "und-Brai",
	// Aliases, deprecated and legacy codes, macro languages.
	"iw", "in", "ji", "jw", "mo", "mo-MD", "tl", "fil", "cmn", "yue", "zh-yue", "zh-min-nan", "zh-cmn",
	"zh-guoyu", "zh-hakka", "zh-xiang", "zh-min", "sgn-BE-FR", "sgn-BE-NL", "sgn-CH-DE", "i-klingon",
	"i-default", "i-enochian", "i-mingo", "i-navajo", "i-lux", "i-ami", "i-bnn", "i-hak", "i-pwn", "i-tao",
	"i-tay", "i-tsu", "no-bok", "no-nyn", "art-lojban", "cel-gaulish", "en-GB-oed", "en-US-POSIX", "root",
	"aar", "deu", "ger", "fra", "fre", "eng", "zho", "chi", "spa", "swe", "rus", "jpn", "kor", "hun", "dan",
	"nld", "dut", "ara", "heb", "hbs", "kin", "yid", "cor", "gsw", "sh-Cyrl", "sh-RS", "nb", "nn", "ckb-IQ",
	"qaa", "qtz", "qzz", "zxx", "mis", "mul", "abc", "zzz", "und-x-private", "en-x-private",
	"de-u-co-phonebk", "fi-u-co-standard", "sv-u-co-standard", "ln-u-co-phonetic", "zh-u-co-stroke",
	"zh-Hant-u-co-pinyin", "en-u-kn-true", "de-u-ks-level1", "en-u-rg-gbzzzz", "en-US-u-rg-gbzzzz",
	"de-1901", "de-1996", "de-CH-1901", "sl-rozaj", "sl-rozaj-biske-1994", "en-scotland", "es-ES-valencia",
	"en-t-fr-est", "fr-est", "en-Latn-US-fonipa", "zh-Latn-pinyin", "ja-Latn-hepburn",
	// Case, separators, M.49 codes and extensions.
	"EN", "En-us", "EN_us", "de_DE", "zh_Hant_TW", "sr_Latn", "EN_CYRL", "es_419", "en-840", "en-826",
	"en-999", "en-000", "de-276", "en-u-co-phonebk-ca", "en-u-nu-arabic-co-phonebk-ca", "en-u-cu-xau-co",
	"en-u-cu-xau-co-phonebk-cu-xau", "en-US-u-cu-xau-cu-eur", "en-t-en-Cyrl-NL-fonipa", "en-9-aa-0-aa-z-bb-x-a",
	"en-c_cc-b-bbb-a-aaa", "en-a-bbb-a-ccc", "de-u-attr-co-phonebk", "de-u-zzz-aaa",
	// Garbage.
	"-", "--", "_", "x", "x-", "a", "1", "12", "123", "1234", "en-", "-en", "en--US", "en.", "en.-en",
	"enne-", "xy", "en-ZY", "ac", "AC", "aa-Uuuu", "aa-AB", "ac-u", "ac-u-ca", "ac-u-ca-co-pinyin", "noob",
	"toolongtag", "en-toolongsubtag", "en-US-toolongsubtag", "klingon", "english", "German", "deutsch",
	"en@US", "en US", "en/US", "*", "ä", "日本", "en-ä", "x-a-tooManyChars-c-d", "a-tooManyChars-c-d",
	"en-v-c", "en-v-abc-x", "en-u-c", "en-t-abcd", "en-Latn-US-en", "de-1902", "en-u-co-phonebook",
	"qaa-Qaaa-QM-x-southern", "en-GB-u-rg-uszzzz-x-y", "en-u-va-posix", "und-u-va-posix", "und-x-a",
}

func textLangBase() (any, error) {
	tags := append([]string{}, textLanguageTags...)
	for a := 'a'; a <= 'z'; a++ {
		for b := 'a'; b <= 'z'; b++ {
			tags = append(tags, string([]rune{a, b}))
		}
	}
	return langBaseCases(tags), nil
}

func langBaseCases(tags []string) []textLangBaseCase {
	var out []textLangBaseCase
	seen := map[string]bool{}
	for _, s := range tags {
		if seen[s] {
			continue
		}
		seen[s] = true
		c := textLangBaseCase{Tag: s, LangBase: tok.LangBase(s)}
		t, err := language.Parse(s)
		if err != nil {
			c.ParseError = err.Error()
		} else {
			c.String = t.String()
			c.IsUnd = t == language.Und
			b, conf := t.Base()
			c.Base, c.Confidence = b.String(), conf.String()
		}
		out = append(out, c)
	}
	return out
}

// The pieces random tags are built from, by position.
var (
	textRandomLangs = []string{"en", "de", "zh", "sr", "sh", "no", "nb", "iw", "mo", "tl", "und", "cmn", "yue",
		"gsw", "fr", "ca", "sl", "ja", "pt", "es", "ar", "qaa", "zzz", "abc", "EN", "Zh", "root", "x", "i", "a"}
	textRandomScripts = []string{"Latn", "Cyrl", "Hant", "Hans", "Qaai", "Zzzz", "Arab", "Jpan", "latn", "LATN",
		"Abcd"}
	textRandomRegions = []string{"US", "GB", "DE", "TW", "HK", "CN", "BR", "ZZ", "QO", "EU", "001", "419", "150",
		"840", "000", "999", "us", "AA", "ZY", "DD", "MD", "SU", "XK"}
	textRandomVariants = []string{"1901", "1996", "valencia", "rozaj", "biske", "1994", "fonipa", "posix",
		"scotland", "1902", "pinyin", "hepburn"}
	textRandomExts = []string{"u-co-phonebk", "u-co-pinyin", "u-co-stroke", "u-va-posix", "u-rg-uszzzz",
		"u-rg-gbzzzz", "u-rg-uszz", "u-ka-shifted", "u-kn-true", "u-ks-level1", "u-nu-arabic", "u-cu-xau",
		"u-attr", "u-abc-def", "u-co", "u", "t-en", "t-fr-est", "t-t0-abcd", "x-private", "x-a", "a-bbb", "z-cc",
		"9-aa", "u-cu-eur", "u-co-phonebk-co-pinyin"}
	textRandomNoise = []string{"", "!", ".", "ä", "abcdefghi", "-", "_", "x"}
)

func textLangRandom() (any, error) {
	r := rand.New(rand.NewSource(20261020))
	pick := func(xs []string) string { return xs[r.Intn(len(xs))] }
	var tags []string
	for len(tags) < 3000 {
		parts := []string{pick(textRandomLangs)}
		if r.Intn(3) == 0 {
			parts = append(parts, pick(textRandomScripts))
		}
		if r.Intn(2) == 0 {
			parts = append(parts, pick(textRandomRegions))
		}
		for n := r.Intn(4); n > 2; n-- {
			parts = append(parts, pick(textRandomVariants))
		}
		if r.Intn(5) == 0 {
			parts = append(parts, pick(textRandomVariants))
		}
		for n := r.Intn(6); n > 2; n-- {
			parts = append(parts, pick(textRandomExts))
		}
		if r.Intn(10) == 0 {
			i := r.Intn(len(parts) + 1)
			parts = append(parts[:i], append([]string{pick(textRandomNoise)}, parts[i:]...)...)
		}
		sep := "-"
		if r.Intn(10) == 0 {
			sep = "_"
		}
		tags = append(tags, strings.Join(parts, sep))
	}
	return langBaseCases(tags), nil
}

// textRandomRunes are the runes random strings are drawn from: letters and the tailored letters of many
// languages, combining marks of several classes, contraction starters and their continuations (Thai, Lao,
// Tibetan, Kannada, Malayalam, Bengali, Japanese iteration and voicing marks), Hangul syllables and jamo, CJK,
// digits of several scripts, ligatures and compatibility characters.
var textRandomRunes = []rune("aAbBcCdDeEhHlLnNoOsSzZ019 -.,'" +
	"äÄåÅöÖøØæÆüÜßéÉèêëçñčšžłıİőűÿœŒǆǄǅ" +
	"\u0300\u0301\u0302\u0308\u030A\u0316\u0323\u0327\u0328\u0335\u0345\u035E\u035F\u0591\u05B0\u05BC" +
	"กขคเแโใไ\u0E31\u0E38\u0E48\u0E4Cເກ\u0EB8\u0F40\u0F71\u0F72\u0F74\u0F80\u0F90\u0FB5" +
	"\u0C95\u0CC6\u0CC2\u0CD5\u0CCA\u0D15\u0D46\u0D3E\u0D57\u0995\u09C7\u09BE\u09D7" +
	"かきカキｶﾞゝゞヽヾー\u3099\u309A가각힣각ᄒ中日汉漢一𠀀㐀豈" +
	"٠١٢०१२０１２₀₁₂①②፩፪ﬁﬀ㎏½²ⅷ℡ﷺ𝔄ⅠⅰЖжЁёЙй")

func textCollateRandom() (any, error) {
	r := rand.New(rand.NewSource(20261020))
	var corpus []string
	for len(corpus) < 600 {
		n := 1 + r.Intn(8)
		rs := make([]rune, n)
		for i := range rs {
			rs[i] = textRandomRunes[r.Intn(len(textRandomRunes))]
		}
		corpus = append(corpus, string(rs))
	}
	langs := []string{"en", "de", "da", "hu", "ja", "ko", "th", "lo", "zh", "kn", "ml", "bn", "si", "my", "km",
		"en-u-kn-true", "ja-u-kn-true", "en-u-ka-shifted-ks-level4", "en-u-ka-posix-ks-level4",
		"en-u-ka-blanked", "fr-u-kb-true", "en-u-kc-true-ks-level1", "en-u-ks-identic"}
	var out []textCollateKeyCase
	for _, lang := range langs {
		cl, ok := dgraphCollator(lang)
		buf := &collate.Buffer{}
		for _, s := range corpus {
			key := append([]byte{}, cl.KeyFromString(buf, s)...)
			buf.Reset()
			out = append(out, textCollateKeyCase{Lang: lang, Text: s, ParseOK: ok, Key: hex.EncodeToString(key),
				LangBase: tok.LangBase(lang)})
		}
	}
	return out, nil
}

// textCollateLanguages are the tags the full corpus is keyed with; every locale with a tailoring of its own is
// keyed with the short corpus.
var textCollateLanguages = []string{
	"en", "de", "fr", "es", "sv", "ru", "zh", "ja", "ko", "hu", "da", "pl", "hi", "nl", "it", "pt", "fi", "no",
	"tr", "cs", "ar", "he", "el", "th", "vi", "uk", "sr", "ln", "is", "lt",
	"de-u-co-phonebk", "fi-u-co-standard", "sv-u-co-standard", "zh-u-co-stroke", "zh-Hant-u-co-pinyin",
	"zh-Hant", "zh-TW", "zh-HK", "sr-Latn", "en-US-u-va-posix", "es-u-co-trad", "de-AT", "fr-CA", "en-GB",
	// The collation options a tag carries.
	"en-u-kn-true", "en-u-kn-false", "de-u-kn-true-ks-level1", "en-u-ks-level1", "en-u-ks-level2",
	"en-u-ks-level3", "en-u-ks-level4", "en-u-ks-identic", "en-u-ka-shifted", "en-u-ka-blanked",
	"en-u-ka-posix", "en-u-ka-shifted-ks-level4", "en-u-ka-posix-ks-level4", "fr-u-kb-true", "fr-CA-u-kb-false",
	"en-u-kc-true", "en-u-kc-true-ks-level1", "en-u-kn-true-ka-shifted-ks-level4",
	// Tags Dgraph replaces by English, or that match no tailoring.
	"", "x!", "und", "xx", "sh", "iw", "x-klingon", "gsw", "hr", "en-GB-u-rg-uszzzz",
}

// textCollateCorpus covers the scripts and cases the keys must reproduce: accents, case, ligatures, digits,
// punctuation, CJK, Hangul, combining marks in and out of canonical order, contractions and expansions.
var textCollateCorpus = []string{
	"", " ", "a", "A", "b", "B", "z", "Z", "ab", "aB", "Ab", "AB", "abc", "ABC", "Abc", "aBc",
	"Äpfel", "Apfel", "apfel", "Ärger", "Arger", "Zebra", "zebra", "Straße", "Strasse", "strasse", "Müller",
	"Mueller", "Muller", "Öl", "Ol", "Übel", "Ubel", "Österreich",
	"côte", "coté", "côté", "cote", "été", "ete", "élève", "Élève", "œuvre", "oeuvre", "Œuvre",
	"ñandú", "nandu", "chico", "cuna", "llama", "luz", "lz", "ch", "cz", "ll",
	"å", "ä", "ö", "ø", "æ", "aa", "Ångström", "Zürich", "Åse", "Øl", "Æble", "Aalborg", "ü", "y", "w", "v",
	"cs", "csak", "cukor", "dzs", "gy", "ly", "ny", "sz", "ty", "zs", "ccs", "ggy", "Ő", "ű", "ő",
	"ёж", "еж", "Ёлка", "Елка", "жук", "Яблоко", "яблоко", "й", "и",
	"chata", "hrad", "cizí", "čaj", "řeka", "ł", "l", "ı", "i", "I", "İstanbul", "istanbul", "Istanbul", "ğ", "ş",
	"中文", "日本語", "汉字", "漢字", "一", "丁", "七", "爸爸", "妈妈", "儿子", "女儿", "𠀀", "㐀", "豈", "鿿", "\U0002A700",
	"한국어", "한글", "가", "각", "갂", "힣", "ㄱ", "ᄀ", "가", "ᅡ", "ᆨ",
	"ひらがな", "カタカナ", "ｶﾀｶﾅ", "ｷﾞ", "ー", "ゝ", "ゞ", "かー", "きゃ", "キャ", "ㇰ",
	"Ｆｕｌｌ", "ｆｕｌｌ", "full", "เก", "กา", "แก่", "ก", "ข",
	"عربي", "עברית", "Ωμέγα", "ωμεγα", "ά", "ᾳ", "हिन्दी", "क्ष", "Việt Nam", "ỹ", "ǖ", "Ǖ",
	"é", "é", "ȩ́", "ȩ́", "ȩ́", "ä", "ä", "ǫ́",
	"á̖", "á̖", "́a", "́", "̖́", "á̵",
	"a" + strings.Repeat("́", 35), "a" + strings.Repeat("̖́", 20) + "b",
	"ﬁ", "fi", "ﬀ", "ﬃ", "Æ", "ǆ", "Ǆ", "ǅ", "dž", "lj", "nj", "Lj", "㎏", "½", "²", "ⅷ", "①", "℡", "ﷺ", "𝔄", "ℌ",
	"0", "1", "9", "10", "2", "02", "007", "123", "12.5", "1,000", "١٢٣", "１２３", "₁₂", "x9y10", "a2", "a10",
	"A-1", "A-2", "A-12", "A-0001", "000", "00", "0001a",
	"-", "_", ".", ",", "!", "?", "@", "#", "'", "\"", "(a)", "a b", "a-b", "a_b", "a\tb", "a\nb", "a.b",
	"😀", "👍🏽", "🇩🇪", "\U0001FAE8", "\x00", "\x7f", "\u200b", "\u00ad", "\ufeff", "\uffff", "\ue000",
	"\U0010FFFD", "͸", "ཀྵ", "ཱི", "ཱི", "ೊ", "ೋ", "เก",
	"ll", "dd", "rh", "〮", "〯", "a͞", "á͟", "á͟b",
	strings.Repeat("abc", 50), strings.Repeat("ä", 40),
}

// textCollateShortCorpus is keyed for every locale with a tailoring.
var textCollateShortCorpus = []string{
	"", "a", "A", "ä", "å", "ö", "ø", "æ", "ch", "ll", "cs", "ñ", "ß", "ž", "ı", "ё", "ж", "中", "日", "가", "ア",
	"é", "é", "ﬁ", "10", "2", "-", " ", "abc", "Zebra", "ก", "ع", "ש", "ω", "ह", "ক", "ა", "ա", "አ", "ʻ",
}

type textCollateKeyCase struct {
	Lang     string `json:"lang"`
	Text     string `json:"text"`
	ParseOK  bool   `json:"parse_ok"` // language.Parse succeeded; Dgraph keys with English otherwise
	Key      string `json:"key"`      // hex
	LangBase string `json:"lang_base"`
}

// dgraphCollator builds the collator as Dgraph's tok.GetTokenizerForLang does for the exact tokenizer.
func dgraphCollator(lang string) (*collate.Collator, bool) {
	tag, err := language.Parse(lang)
	if err != nil {
		tag, _ = language.Parse("en")
	}
	return collate.New(tag), err == nil
}

func textCollateKeys() (any, error) {
	var out []textCollateKeyCase
	add := func(lang string, corpus []string) error {
		cl, ok := dgraphCollator(lang)
		buf := &collate.Buffer{}
		base := tok.LangBase(lang)
		var exact tok.Tokenizer
		if lang != "" {
			exact = tok.GetTokenizerForLang(tok.ExactTokenizer{}, lang)
		}
		for _, s := range corpus {
			key := append([]byte{}, cl.KeyFromString(buf, s)...)
			buf.Reset()
			// Cross-check with Dgraph's exact tokenizer: base language, 0x1f, key.
			if exact != nil {
				toks, err := exact.Tokens(s)
				if err != nil {
					return err
				}
				want := base + "\x1f" + string(key)
				if len(toks) != 1 || toks[0] != want {
					return fmt.Errorf("exact token of %q in %q is %x, want %x", s, lang, toks, want)
				}
			}
			out = append(out, textCollateKeyCase{Lang: lang, Text: s, ParseOK: ok, Key: hex.EncodeToString(key),
				LangBase: base})
		}
		return nil
	}
	for _, lang := range textCollateLanguages {
		if err := add(lang, textCollateCorpus); err != nil {
			return nil, err
		}
	}
	for _, lang := range collate.Supported() {
		if err := add(lang.String(), textCollateShortCorpus); err != nil {
			return nil, err
		}
	}
	return out, nil
}

type textCollateCompareCase struct {
	Lang string `json:"lang"`
	// Texts is the corpus; Compare[i] holds CompareString(Texts[i], Texts[j]) for every j as '<', '=' or '>'.
	Texts   []string `json:"texts"`
	Compare []string `json:"compare"`
	// Sorted is the corpus sorted stably by CompareString.
	Sorted []string `json:"sorted"`
}

var textCompareCorpus = []string{
	"", "a", "A", "á", "Á", "á", "ä", "Ä", "ae", "æ", "b", "B", "c", "č", "ch", "Ch", "cs", "d", "dz",
	"e", "é", "è", "ê", "ë", "o", "ö", "oe", "ø", "ss", "ß", "z", "å", "aa", "ö", "y", "ü", "1", "10", "2",
	"-", " ", "a-b", "ab", "aB", "Ab", "中", "日", "가", "ア", "あ", "ё", "е", "ж", "ﬁ", "fi",
}

func textCollateCompare() (any, error) {
	langs := []string{"en", "de", "de-u-co-phonebk", "fr", "fr-CA", "es", "es-u-co-trad", "sv", "da", "fi",
		"hu", "cs", "ru", "zh", "zh-Hant", "ja", "ko", "en-u-ks-level1", "en-u-ks-level2", "en-u-ks-identic",
		"en-u-ka-shifted", "en-u-ka-posix", "en-u-kn-true", "en-u-kc-true-ks-level1", "fr-u-kb-true", "x!"}
	var out []textCollateCompareCase
	for _, lang := range langs {
		var cl *collate.Collator
		// As Dgraph's types.Sort: no collator when the tag does not parse.
		if tag, err := language.Parse(lang); err == nil {
			cl = collate.New(tag)
		} else {
			continue
		}
		c := textCollateCompareCase{Lang: lang, Texts: textCompareCorpus}
		for _, a := range textCompareCorpus {
			var row strings.Builder
			for _, b := range textCompareCorpus {
				row.WriteByte("<=>"[cl.CompareString(a, b)+1])
			}
			c.Compare = append(c.Compare, row.String())
		}
		c.Sorted = append([]string{}, textCompareCorpus...)
		sort.SliceStable(c.Sorted, func(i, j int) bool { return cl.CompareString(c.Sorted[i], c.Sorted[j]) < 0 })
		out = append(out, c)
	}
	return out, nil
}

// textInvalidFragments are byte sequences a Go string holds that are not valid UTF-8: lone continuation bytes,
// overlong encodings, surrogates, code points past U+10FFFF and the old five- and six-byte forms, bytes that never
// occur in UTF-8, sequences cut short at the end of the input, and sequences broken by a byte that does not
// continue them.
var textInvalidFragments = []string{
	"\x80", "\xbf", "\x80\x80", "\x80\xbf\x80",
	"\xc0\x80", "\xc1\xbf", "\xe0\x80\x80", "\xe0\x9f\xbf", "\xf0\x80\x80\x80", "\xf0\x8f\xbf\xbf",
	"\xed\xa0\x80", "\xed\xbf\xbf", "\xed\xa0\xbd\xed\xb8\x80",
	"\xf4\x90\x80\x80", "\xf5\x80\x80\x80", "\xf7\xbf\xbf\xbf", "\xf8\x88\x80\x80\x80", "\xfc\x84\x80\x80\x80\x80",
	"\xfe", "\xff", "\xfe\xff", "\xff\xfe", "\xff\xff\xff",
	"\xc3", "\xe2\x82", "\xe2", "\xf0\x9f\x98", "\xf0\x9f", "\xf0", "\xe4\xb8", "\xcc",
	"\xc3(", "\xe2(\xa1", "\xe2\x82(", "\xf0(\x8c\xbc", "\xf0\x90(\xbc", "\xf0\x90\x8c(", "\xe0\xa0x",
}

// textInvalidCorpus places the fragments at the start, in the middle and at the end of valid text, and inside the
// sequences the collator reads ahead over: contractions (Czech ch, Hungarian cs, Danish aa and å, Spanish ll,
// Thai and Japanese), combining marks in and out of canonical order, digits under the numeric option, Hangul and
// CJK. U+FFFD itself is in it too: it has a weight of its own, unlike an invalid byte.
func textInvalidCorpus() []string {
	out := []string{"\uFFFD", "a\uFFFDb", "\uFFFD\xff", "\xff\uFFFD", "e\xcc", "a\u0316\xcc", "\u0E40\xe0\xb8",
		"\u30AB\xe3\x83", "\u4E2D\xe4\xb8", "Stra\u00DFe\xc3", "\xff\xff\xff\xff" + strings.Repeat("\xfe", 40),
		strings.Repeat("a\x80", 30), "a" + strings.Repeat("\u0301", 35) + "\x80", "\x00\xff", "\xff\x00"}
	contexts := []func(f string) string{
		func(f string) string { return f },
		func(f string) string { return "a" + f },
		func(f string) string { return f + "a" },
		func(f string) string { return "a" + f + "b" },
		func(f string) string { return "\u00E4" + f + "z" },
		func(f string) string { return "ch" + f },
		func(f string) string { return "c" + f + "h" },
		func(f string) string { return "c" + f + "s" },
		func(f string) string { return "a" + f + "a" },
		func(f string) string { return "a\u0301" + f },
		func(f string) string { return "a" + f + "\u0301" },
		func(f string) string { return "a\u0316" + f + "\u030A" },
		func(f string) string { return "a\u0316\u030A" + f },
		func(f string) string { return "1" + f + "2" },
		func(f string) string { return "12" + f + "34" },
		func(f string) string { return "\uAC00" + f + "\u1161" },
		func(f string) string { return "\u0E40" + f + "\u0E01" },
		func(f string) string { return "\u30AB" + f + "\u30FC" },
		func(f string) string { return "\u4E2D" + f + "\u6587" },
		func(f string) string { return f + f },
	}
	for _, f := range textInvalidFragments {
		for _, c := range contexts {
			out = append(out, c(f))
		}
	}
	return out
}

// textInvalidLanguages are the tags the invalid corpus is keyed and compared with: tailorings with contractions,
// and the collation options (numeric, shifted, blanked, backwards secondaries, case level, identical level).
var textInvalidLanguages = []string{"en", "de", "da", "cs", "hu", "es-u-co-trad", "ja", "ko", "th", "zh", "fr-CA",
	"en-u-kn-true", "en-u-ka-shifted-ks-level4", "en-u-ka-blanked", "en-u-ks-identic", "en-u-kc-true-ks-level1",
	"x!"}

// textInvalidRandom draws strings from runes, invalid fragments and arbitrary bytes, with a fixed seed.
func textInvalidRandom(seed int64, n int) []string {
	r := rand.New(rand.NewSource(seed))
	var out []string
	for len(out) < n {
		var b strings.Builder
		for k := 1 + r.Intn(8); k > 0; k-- {
			switch x := r.Intn(10); {
			case x < 6:
				b.WriteRune(textRandomRunes[r.Intn(len(textRandomRunes))])
			case x < 9:
				b.WriteString(textInvalidFragments[r.Intn(len(textInvalidFragments))])
			default:
				b.WriteByte(byte(0x80 + r.Intn(0x80)))
			}
		}
		out = append(out, b.String())
	}
	return out
}

type textInvalidKeyCase struct {
	Lang     string `json:"lang"`
	Text     string `json:"text"` // hex
	ParseOK  bool   `json:"parse_ok"`
	Key      string `json:"key"` // hex
	LangBase string `json:"lang_base"`
}

// textCollateInvalid keys the invalid corpus and random strings as Dgraph's exact tokenizer does, with
// KeyFromString; Key over the same bytes gives the same key, which the generator checks, as it checks the token.
func textCollateInvalid() (any, error) {
	corpus := textInvalidCorpus()
	for _, s := range corpus {
		if utf8.ValidString(s) && !strings.ContainsRune(s, utf8.RuneError) {
			return nil, fmt.Errorf("%q is valid UTF-8", s)
		}
	}
	// A random byte may complete a fragment, so a random string may be valid.
	corpus = append(corpus, textInvalidRandom(20261021, 600)...)
	var out []textInvalidKeyCase
	for _, lang := range textInvalidLanguages {
		cl, ok := dgraphCollator(lang)
		buf := &collate.Buffer{}
		base := tok.LangBase(lang)
		exact := tok.GetTokenizerForLang(tok.ExactTokenizer{}, lang)
		for _, s := range corpus {
			key := append([]byte{}, cl.KeyFromString(buf, s)...)
			buf.Reset()
			if k := cl.Key(buf, []byte(s)); !bytes.Equal(k, key) {
				return nil, fmt.Errorf("Key(%x) = %x, KeyFromString %x", s, k, key)
			}
			buf.Reset()
			toks, err := exact.Tokens(s)
			if err != nil {
				return nil, err
			}
			if want := base + "\x1f" + string(key); len(toks) != 1 || toks[0] != want {
				return nil, fmt.Errorf("exact token of %x in %q is %x, want %x", s, lang, toks, want)
			}
			out = append(out, textInvalidKeyCase{Lang: lang, Text: hex.EncodeToString([]byte(s)), ParseOK: ok,
				Key: hex.EncodeToString(key), LangBase: base})
		}
	}
	return out, nil
}

// textInvalidCompareCorpus mixes invalid strings with the valid ones they collate beside.
var textInvalidCompareCorpus = []string{
	"", "a", "A", "b", "\u00E4", "z", "ch", "cs", "aa", "\u00E5", "1", "2", "10", "\uFFFD", "\uFFFDa", "\u4E2D", "\uAC00", "\u30AB",
	"\x80", "\xbf", "\xff", "\xfe", "\xff\xfe", "\xfe\xff", "\x80\x80", "\xc0\x80", "\xe0\x80\x80", "\xed\xa0\x80",
	"\xf4\x90\x80\x80", "\xf8\x88\x80\x80\x80", "\xc3", "\xe2\x82", "\xe0\xa0x", "\xe2\x82(", "a\x80", "\x80a",
	"a\x80b", "a\xffb", "ab\x80", "\xc3\xa4\xff", "\xff\xc3\xa4", "c\x80h", "ch\x80", "a\u0301\x80", "a\x80\u0301",
	"1\x802", "12\xff", "\xff12", "\uAC00\x80", "\u4E2D\xff", "e\xcc", "\x00\xff", "\xff\x00", "a\uFFFDb",
}

type textInvalidCompareCase struct {
	Lang string `json:"lang"`
	// Texts is the corpus in hex; Compare[i] holds CompareString(Texts[i], Texts[j]) for every j as '<', '=' or
	// '>'.
	Texts   []string `json:"texts"`
	Compare []string `json:"compare"`
	// Sorted is the corpus in hex, sorted stably by CompareString.
	Sorted []string `json:"sorted"`
}

// textCollateInvalidCompare compares every pair of the corpus, and of random strings, with CompareString, as
// Dgraph's language sort does; Compare over the same bytes agrees, which the generator checks.
func textCollateInvalidCompare() (any, error) {
	random := textInvalidRandom(20261022, 80)
	var out []textInvalidCompareCase
	for _, lang := range textInvalidLanguages {
		// As Dgraph's types.Sort: no collator when the tag does not parse.
		tag, err := language.Parse(lang)
		if err != nil {
			continue
		}
		cl := collate.New(tag)
		for _, corpus := range [][]string{textInvalidCompareCorpus, random} {
			c := textInvalidCompareCase{Lang: lang}
			for _, a := range corpus {
				c.Texts = append(c.Texts, hex.EncodeToString([]byte(a)))
				var row strings.Builder
				for _, b := range corpus {
					res := cl.CompareString(a, b)
					if r := cl.Compare([]byte(a), []byte(b)); r != res {
						return nil, fmt.Errorf("Compare(%x, %x) = %d, CompareString %d", a, b, r, res)
					}
					row.WriteByte("<=>"[res+1])
				}
				c.Compare = append(c.Compare, row.String())
			}
			sorted := append([]string{}, corpus...)
			sort.SliceStable(sorted, func(i, j int) bool { return cl.CompareString(sorted[i], sorted[j]) < 0 })
			for _, s := range sorted {
				c.Sorted = append(c.Sorted, hex.EncodeToString([]byte(s)))
			}
			out = append(out, c)
		}
	}
	return out, nil
}
