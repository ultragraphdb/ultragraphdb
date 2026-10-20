// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

package main

// The text corpus of ultragraphdb_core_tok's golden vectors (tok.go, tok_analysis.go): sentences per language for
// every language Dgraph stems, removes stop words from or splits on spaces, the sentences of Dgraph's
// tok/tok_test.go, text exercising each word-break class bleve's segmenter distinguishes, and random strings drawn
// with a fixed seed.

import (
	"math/rand"
	"sort"
	"strings"
)

// tokLangSentences are sentences per language, keyed by the language tag Dgraph is given.
var tokLangSentences = map[string][]string{
	"ar": {"كان الأطفال يلعبون بسعادة في حدائق المدينة الكبيرة.", "والكتاب بالقلم للمدرسة وللطلاب والمعلمين",
		"الكاتبون يكتبون كتاباتهم في المكتبات الجامعية"},
	"bg": {"Децата играеха весело в големите градини на града.", "Той и тя са в къщата."},
	"ca": {"Els nens jugaven alegrement als grans jardins de la ciutat.", "Ells i elles són a casa."},
	"ckb": {"منداڵەکان بە خۆشییەوە لە باخچە گەورەکانی شارەکەدا یارییان دەکرد.",
		"کتێبەکان و قوتابخانەیەکی و ماڵەکەمان"},
	"cs": {"Děti si vesele hrály ve velkých zahradách města.", "Já a ty jsme doma."},
	"da": {"Børnene legede glade i de store haver i byen.", "Hun havde været hjemme hele dagen og læste bøgerne."},
	"de": {"Katzen und Auffassung und Auffassung", "Die Katzen und die Hunde spielten fröhlich im großen Garten.",
		"Häuser, Bäume, Straßen und Gärten; ÄÖÜ äöü ß ẞ STRASSE"},
	"el": {"Τα παιδιά έπαιζαν χαρούμενα στους μεγάλους κήπους της πόλης.", "ΟΔΟΣ ΟΔΟΣ. ΣΣ Σ σοφός"},
	"en": {"Stemming works!", "Quick brown fox", "the quick brown foxes jump over the big dogs",
		"Our chief weapon is surprise...surprise and fear...fear and surprise...." +
			"Our two weapons are fear and surprise...and ruthless efficiency.... " +
			"Our three weapons are fear, surprise, and ruthless efficiency...",
		"Running quickly, the runners' relational organizations generalized everything happily.",
		"The quick brown fox jumps over the lazy dog. This is a test sentence with multiple words."},
	"es": {"deseándoles muchas alegrías a las señoritas y los señores programadores de Dgraph",
		"Los niños estaban corriendo rápidamente por las calles de las ciudades."},
	"eu": {"Haurrak pozik jolasten ziren hiriko lorategi handietan.", "Ni eta zu etxean gaude."},
	"fa": {"کودکان با شادی در باغ\u200cهای بزرگ شهر بازی می\u200cکردند.", "من و تو در خانه هستیم."},
	"fi": {"Lapset leikkivät iloisesti kaupungin suurissa puutarhoissa.", "Minä ja sinä olemme kotona taloissamme."},
	"fr": {"Les enfants jouaient joyeusement dans les jardins publics.",
		"L'organisation nationale des travailleurs internationaux généralement inquiétante.",
		"chevaux, châteaux, normalisation, majestueusement, ambassadrice, vérificatrice"},
	"ga": {"Bhí na páistí ag súgradh go sona i ngairdíní móra na cathrach.", "Tá mé agus tú sa bhaile."},
	"gl": {"Os nenos xogaban alegremente nos grandes xardíns da cidade.", "Eu e ti estamos na casa."},
	"hi": {"बच्चे शहर के बड़े बगीचों में खुशी से खेल रहे थे।", "लड़कियाँ किताबें पढ़ेंगी और लड़के खेलेंगे"},
	"hu": {"A gyerekek vidáman játszottak a város nagy kertjeiben.", "Én és te otthon vagyunk a házainkban."},
	"hy": {"Երեխաները ուրախ խաղում էին քաղաքի մեծ այգիներում։", "Ես և դու տանն ենք։"},
	"id": {"Anak-anak bermain dengan gembira di taman-taman besar kota.", "Saya dan kamu ada di rumah."},
	"it": {"I bambini giocavano felicemente nei giardini della città.", "Le ragazze e i ragazzi cantavano canzoni."},
	"nl": {"De kinderen speelden vrolijk in de grote tuinen van de stad.", "Ik en jij zijn thuis in de huizen."},
	"no": {"Barna lekte lykkelig i de store hagene i byen.", "Jeg og du er hjemme i husene våre."},
	"pt": {"As crianças brincavam alegremente nos jardins das cidades.", "Os meninos e as meninas cantavam canções."},
	"ro": {"Copiii se jucau fericiți în grădinile mari ale orașului.",
		"Copiii se jucau fericiţi în grădinile mari ale oraşului."},
	"ru":  {"Дети весело играли в больших садах города.", "Я и ты дома в наших домах."},
	"sv":  {"Barnen lekte glatt i de stora trädgårdarna i staden.", "Jag och du är hemma i husen."},
	"tr":  {"Çocuklar şehrin büyük bahçelerinde neşeyle oynuyorlardı.", "Ben ve sen evlerimizdeyiz. İSTANBUL ıi"},
	"zh":  {"他是一个薪水很高的商人", "他是 一个好人", "第一轮 第二轮 第一轮", "我爱北京天安门。Dgraph是图数据库, 版本25"},
	"ja":  {"彼は大きな給与を持つ実業家です", "カタカナとひらがなと漢字。ｶﾀｶﾅ テスト"},
	"ko":  {"그는 큰 급여를 가진 사업가입니다.", "한국어 텍스트 처리 漢字 섞임"},
	"th":  {"ภาษาไทยไม่มีช่องว่าง ระหว่างคำ", "สวัสดี ครับ"},
	"lo":  {"ພາສາລາວ ບໍ່ມີ ຍະຫວ່າງ"},
	"my":  {"မြန်မာဘာသာ စကား"},
	"bo":  {"བོད་ཡིག སྐད་ཡིག"},
	"km":  {"ភាសាខ្មែរ គ្មាន ចន្លោះ"},
	"kxm": {"ភាសាខ្មែរ ខាងជើង"},
}

// tokMixedCorpus exercises the segmenter's word-break classes, the lowercase and NFKC filters, and Dgraph's
// tokenizers; every string is tokenized under every language below.
var tokMixedCorpus = []string{
	"", " ", "   ", "Tokenizer works works!", "Dgraph rocks!", "Grüße", "Barack Obama made Obamacare",
	"3.5 1,000,000 1.2.3 $42.00 3rd 2nd 4'5 1'000", "don't O'Neil can't won't 'quoted' rock'n'roll",
	"e-mail foo@bar.com http://dgraph.io/path?q=1#x a.b.c a:b", "snake_case CamelCase __init__ a_1 _ __ x‿y",
	"שלום עולם צה\"ל א' ב\"ג", "カタカナ・テスト ｶﾀｶﾅ ﾃｽﾄ カ_タ", "ひらがな ゝゞ", "漢字かな交じり文",
	"한국어 텍스트 한글", "I ❤ 🍕 👍🏽 🇩🇪🇺🇸🇫 👨\u200d👩\u200d👧",
	"été café ñ ñ Å Å", "ﬁnancial ﬂow ﬀ ŉ ǅ", "ＡＢＣ１２３ ｱｲｳ",
	"Ⅻ ⅻ ①②③ ½ ² ㎏ ℃", "ΟΔΟΣ ΣΣ Σ ΣΑ", "İSTANBUL ıi Iİ", "STRASSE straße ẞ", "ȺȾ Ⱥbc aȺ ȺΣ",
	"tab\tnew\nline\r\ncr\rend\u0085nel\u2028ls\u2029ps\vvt\fff", "a\x00b", "abc\xffdef", "\xff\xfeabc", "日本\xff語",
	"abc \xc3 def", "ひら\xffがな", "カタ\xc3カナ", "\xe2\x82", "1\xff2",
	"a" + strings.Repeat("́", 40) + " b", "a" + strings.Repeat("̣́", 20) + " c",
	"x" + strings.Repeat("̕", 33), strings.Repeat("abcdefghij", 31),
	"١٢٣ ۴۵۶ ०१२ ๑๒๓ 12٣", "soft\u00adhyphen zero\u200bwidth word\u2060joiner\ufeffbom",
	"supercalifragilisticexpialidocious antidisestablishmentarianism pneumonoultramicroscopicsilicovolcanoconiosiss",
	"running quickly", "the quick brown fox", "hello", "hello world", "single", "one two three four",
	"the quick brown fox jumps", "quick fox", "quick brown fox", "sentence multiple words", "lazy dog test sentence",
	"the a an of", "The THE tHe", "MIXED case Words AND numbers 42", "ǈ ǋ ǲ ᾈ ᾙ", "Ꭰ ꭰ Ᏸ",
	"𐐀𐐁 𐐨𐐩", "𝐀𝐁𝐂 𝟏𝟐", "a·b l·l ŀl", "x\u200cy x\u200dy", "̀abc", "\u200d", "א׳",
	"　full　width　space", "a b", "a.b,c;d:e", "1.a a.1 1,a", "_1_ _a_ a__b",
	"ǰ ẖ ẘ", "ß ŉ ǰ ΐ ΰ և ﬀ ﬃ ﬆ", "Ǟ ā̈",
	"ᄀᄀᄀ각ᆨᆨ", "\U0001F1E6\U0001F1E7\U0001F1E8", "क्षत्रिय हिंदी", "ไทย", "ᜀᜁ", "ⓐⓑⓒ ⒜ ⑴",
	"0x1F 0b101 1e10 1.5e-3 -42 +7", "ﾊﾟ ﾊﾞ ｶﾞ", "㋿ ㍻ ㌀", "ꀀ ꀁ", "⺀⺁ ⼀", "〆々〇",
}

// tokRandomStrings draws n strings from runes of every word-break class bleve's segmenter distinguishes, with a
// fixed seed.
func tokRandomStrings(seed int64, n int) []string {
	pools := [][]rune{
		[]rune("abcxyzABCXYZ"), []rune("0123456789"), []rune(" \t"), []rune(".,;:'\"!?-_/@#"),
		[]rune("àéîõüßÆøÿ"), []rune("αβγΣσςΩ"), []rune("абвЖЯ"), []rune("אבגדה"), []rune("ابتثج"),
		[]rune("कखगािु्"), []rune("กขคะา"), []rune("一二三人大中"), []rune("あいうゝ"), []rune("アイウーｱｲ"),
		[]rune("가나다한글"), []rune("̣̀́̈"), []rune("\u200c\u200d\u00ad\u200b\u2060"),
		[]rune("❤🍕👍🏽"), []rune("\U0001F1E6\U0001F1E7\U0001F1E8"), []rune("\r\n\u0085\u2028\v\f"),
		[]rune("·‿⁀＿"), []rune("ﬁﬂＡ１①Ⅻ"), []rune("İȺȾ"), []rune("׳״"),
	}
	r := rand.New(rand.NewSource(seed))
	out := make([]string, 0, n)
	for i := 0; i < n; i++ {
		var b strings.Builder
		k := 1 + r.Intn(24)
		for j := 0; j < k; j++ {
			if r.Intn(60) == 0 {
				b.WriteByte(byte(0x80 + r.Intn(0x80))) // an invalid byte now and then
				continue
			}
			p := pools[r.Intn(len(pools))]
			b.WriteRune(p[r.Intn(len(p))])
		}
		out = append(out, b.String())
	}
	return out
}

// tokCorpus is every string of the corpus, the language sentences in tag order first.
func tokCorpus() []string {
	var out []string
	for _, lang := range tokSortedLangs() {
		out = append(out, tokLangSentences[lang]...)
	}
	out = append(out, tokMixedCorpus...)
	return append(out, tokRandomStrings(20261020, 400)...)
}

func tokSortedLangs() []string {
	langs := make([]string, 0, len(tokLangSentences))
	for l := range tokLangSentences {
		langs = append(langs, l)
	}
	sort.Strings(langs)
	return langs
}
