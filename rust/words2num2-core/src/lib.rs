//! Pure-Rust core for words2num2 — the inverse of num2words2.
//!
//! # Why this is small
//!
//! 119 of words2num2's 120 locales never had a hand-written parser. They use
//! `Words2Num_Base`, which materialises a reverse lookup table by calling
//! `num2words` across `LOOKUP_RANGE` (`range(-1, 10001)`) — 10,002 renders —
//! and then does a dict hit. Only `en` is hand-written.
//!
//! So the port is: the generic table backend + `_normalize` + the `en`
//! grammar parser. The table is built by calling the Rust num2words core
//! (`num2words2-core`) directly, which is where the speedup comes from.
//!
//! This crate has **no** PyO3 dependency: it is the pure-Rust engine. The
//! `words2num2-py` crate is a thin PyO3 binder over the public API here.

use num_bigint::BigInt;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

pub mod w2n_formats;
pub mod w2n_lang_en;
pub mod w2n_sentence;

/// The public single-token entry point (`words2num2.words2num`), re-exported at
/// the crate root. Its dispatch — `_resolve_lang`, the en-vs-reverse-table
/// choice, and the `to` mode selection — lives in [`w2n_sentence`].
pub use w2n_sentence::words2num;

/// Python's `Words2Num_Base.LOOKUP_RANGE`.
const LOOKUP_LO: i64 = -1;
const LOOKUP_HI: i64 = 10001;

/// Error from the reverse-table backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupError {
    /// num2words2 has no backend for this locale key
    /// (Python raised `NotImplementedError(lang)`).
    NotImplemented(String),
}

/// Port of the tail of `Words2Num_Base._normalize` — the pure ASCII-shaped
/// rewriting that follows NFKD decomposition + combining-mark stripping.
///
/// ```python
/// text = text.lower().replace("_", " ")
/// text = re.sub(r"(?<=[a-z])-(?=[a-z])", " ", text)
/// text = re.sub(r"[,;:!\?\"']", " ", text)
/// text = re.sub(r"\.(?!\d)", " ", text)
/// text = re.sub(r"\s+", " ", text).strip()
/// ```
pub fn normalize_tail(decomposed: &str) -> String {
    let lowered = decomposed.to_lowercase().replace('_', " ");
    let chars: Vec<char> = lowered.chars().collect();
    let mut out = String::with_capacity(lowered.len());

    for (i, &c) in chars.iter().enumerate() {
        match c {
            // (?<=[a-z])-(?=[a-z]) — a hyphen joining two words becomes a
            // space, but a hyphen before a digit is a sign and must survive.
            '-' => {
                let prev_alpha = i > 0 && chars[i - 1].is_ascii_lowercase();
                let next_alpha = chars.get(i + 1).is_some_and(|n| n.is_ascii_lowercase());
                out.push(if prev_alpha && next_alpha { ' ' } else { '-' });
            }
            ',' | ';' | ':' | '!' | '?' | '"' | '\'' => out.push(' '),
            // \.(?!\d) — sentence-final dot goes, decimal point stays.
            '.' => {
                let next_digit = chars.get(i + 1).is_some_and(|n| n.is_ascii_digit());
                out.push(if next_digit { '.' } else { ' ' });
            }
            _ => out.push(c),
        }
    }
    // \s+ -> " ", then strip.
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Pure-Rust port of `Words2Num_Base._normalize`.
///
/// ```python
/// nfkd = unicodedata.normalize("NFKD", text)
/// text = "".join(c for c in nfkd if not unicodedata.combining(c))
/// # ... normalize_tail ...
/// ```
///
/// The NFKD decomposition + combining-mark strip is what makes "trente-deux"
/// match "trente deux". It is done here with the `unicode-normalization`
/// crate rather than round-tripping to Python's `unicodedata`, so the core is
/// self-contained.
pub fn normalize(s: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    let nfkd: String = s.nfkd().collect();
    let stripped: String = nfkd
        .chars()
        .filter(|&c| !unicode_normalization::char::is_combining_mark(c))
        .collect();
    normalize_tail(&stripped)
}

/// One reverse table: `{normalized_words: number}`.
type ReverseTable = HashMap<String, i64>;
/// The per-(lang, kind) cache of reverse tables.
type TableCache = RwLock<HashMap<(String, bool), ReverseTable>>;

/// The reverse tables, built lazily per (lang, kind) exactly as Python does.
fn tables() -> &'static TableCache {
    static T: OnceLock<TableCache> = OnceLock::new();
    T.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Port of `Words2Num_Base._build_table`.
///
/// Python calls `num2words(n, lang, to=kind)` for every n in LOOKUP_RANGE and
/// does `table.setdefault(key, n)` — **first write wins**, so the canonical
/// short form takes precedence over later spellings. That ordering is
/// load-bearing; do not switch to insert-overwrite.
fn build_table(lang: &str, ordinal: bool) -> Result<HashMap<String, i64>, LookupError> {
    let l = num2words2_core::get_lang_by_key(lang)
        .ok_or_else(|| LookupError::NotImplemented(lang.to_string()))?;
    let mut table = HashMap::new();
    for n in LOOKUP_LO..LOOKUP_HI {
        let v = BigInt::from(n);
        let words = if ordinal { l.to_ordinal(&v) } else { l.to_cardinal(&v) };
        let Ok(words) = words else { continue }; // Python swallows every raise
        let key = normalize(&words);
        table.entry(key).or_insert(n);
    }
    Ok(table)
}

/// Port of `Words2Num_Base._lookup` + `to_cardinal`/`to_ordinal`.
///
/// Returns `None` when the text is not in the table, so the Python side can
/// raise `Words2NumError` with its exact message rather than us guessing it.
pub fn lookup(
    lang: &str,
    text: &str,
    ordinal: bool,
    negative_words: &[String],
) -> Result<Option<i64>, LookupError> {
    let mut normalized = normalize(text);
    if normalized.is_empty() {
        return Ok(None);
    }

    let mut sign = 1i64;
    for neg in negative_words {
        if normalized == *neg {
            return Ok(None); // a bare negword is unparseable
        }
        if let Some(rest) = normalized.strip_prefix(&format!("{} ", neg)) {
            sign = -1;
            normalized = rest.to_string();
            break;
        }
    }

    let key = (lang.to_string(), ordinal);
    {
        let t = tables().read().unwrap();
        if let Some(tab) = t.get(&key) {
            return Ok(tab.get(&normalized).map(|v| sign * v));
        }
    }
    let built = build_table(lang, ordinal)?;
    let got = built.get(&normalized).map(|v| sign * v);
    tables().write().unwrap().insert(key, built);
    Ok(got)
}

/// Languages the Rust core can serve (Python's `_RUST.supported_langs()`).
pub fn supported_langs() -> Vec<&'static str> {
    num2words2_core::supported_lang_keys()
}

// ---------------------------------------------------------------------------
// Multi-scale cardinal composition (values above the LOOKUP_RANGE table)
// ---------------------------------------------------------------------------
//
// The reverse table only covers -1..10001, so a spoken number like
// "soixante-neuf mille huit" (69008) has no whole-string entry and the sentence
// walker was left emitting the fragments "69 1000 8". `parse_scaled` restores
// the arithmetic num2words never inverted: it splits on a language's scale
// words (mille=10^3, million=10^6, milliard=10^9) and composes
// `left * scale + right`, recursively. Space-separated languages (fr/es/pt/…)
// gain full thousands/millions support; agglutinative ones (de/nl, where the
// scale is glued into one token) find no split word and fall back to the table
// unchanged — so this never regresses them.

/// Cache of `(scale_word_normalized -> magnitude)` per language, biggest first.
fn scale_cache() -> &'static RwLock<HashMap<String, Vec<(String, i64)>>> {
    static S: OnceLock<RwLock<HashMap<String, Vec<(String, i64)>>>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Scale words for `lang`, biggest magnitude first. Derived by rendering the
/// SINGULAR and PLURAL of 10^6 and 10^3 (n and 2n) via num2words and taking the
/// last token of each: fr → {"millions"/"million" → 10^6, "mille" → 10^3}. Only
/// 10^3 and 10^6 are probed — a language's 10^9 is often a *compound* of these
/// ("mil millones", "mil milhões") and the recursion composes it from the parts,
/// so probing 10^9 directly would mis-map its last token ("millones") to 10^9.
fn scale_words(lang: &str) -> Vec<(String, i64)> {
    if let Some(v) = scale_cache().read().unwrap().get(lang) {
        return v.clone();
    }
    let mut out: Vec<(String, i64)> = Vec::new();
    if let Some(l) = num2words2_core::get_lang_by_key(lang) {
        // (magnitude, [singulier, pluriel]) — le pluriel capte « millions » vs
        // « million » (num2words rend 2×10^6 au pluriel).
        // Échantillons n, 2n, 5n : capte les formes grammaticales du mot
        // d'échelle — singulier (« mille »/« tysiąc »), petit pluriel (« millions »
        // /« tysiące ») ET pluriel génitif slave 5+ (« tysięcy »/« тысяч »).
        // Ordre PETIT → GRAND : le garde anti-collision fixe alors chaque token à
        // sa PLUS PETITE magnitude. Essentiel en échelle longue (es « mil
        // millones » = 10^9 réutilise « millones » = 10^6) : « millones » reste à
        // 10^6, et 10^9 se compose par récursion (mil × millones).
        for (mag, samples) in [
            (1_000i64, [1_000i64, 2_000i64, 5_000i64]),
            (1_000_000i64, [1_000_000i64, 2_000_000i64, 5_000_000i64]),
            (1_000_000_000i64, [1_000_000_000i64, 2_000_000_000i64, 5_000_000_000i64]),
        ] {
            for s in samples {
                if let Ok(words) = l.to_cardinal(&BigInt::from(s)) {
                    let norm = normalize(&words);
                    if let Some(last) = norm.split_whitespace().last() {
                        let w = last.to_string();
                        // Token d'échelle DISTINCT, ≥3 lettres, non numérique.
                        if w.len() >= 3
                            && !w.chars().any(|c| c.is_ascii_digit())
                            && !out.iter().any(|(x, _)| *x == w)
                        {
                            out.push((w, mag));
                        }
                    }
                }
            }
        }
    }
    // Plus grande magnitude d'abord : « million » se scinde avant « mille ».
    out.sort_by(|a, b| b.1.cmp(&a.1));
    scale_cache()
        .write()
        .unwrap()
        .insert(lang.to_string(), out.clone());
    out
}

/// Table hit for a fragment (no sign handling), i.e. a number ≤ 10001.
fn lookup_plain(lang: &str, text: &str) -> Option<i64> {
    lookup(lang, text, false, &[]).ok().flatten()
}

/// Connector/particle words that may sit between number groups and must be
/// stripped from a fragment's edges before lookup: es « y », pt/it « e »,
/// ro « de » (« nouă **de** mii »), etc. Keyed by language prefix.
fn connector_words(lang: &str) -> &'static [&'static str] {
    let base = lang.split(&['_', '-'][..]).next().unwrap_or(lang);
    match base {
        "es" | "gl" => &["y", "e"],
        "pt" | "it" => &["e"],
        "fr" => &["et"],
        "ca" => &["i"],
        "de" => &["und"],
        "nl" | "af" => &["en"],
        "ro" => &["si", "și", "de"],
        "pl" => &["i"],
        "en" => &["and"],
        _ => &[],
    }
}

/// Strip leading/trailing connector tokens from a fragment.
fn trim_connectors<'a>(s: &'a str, conns: &[&str]) -> String {
    let mut toks: Vec<&str> = s.split_whitespace().collect();
    while toks.first().is_some_and(|t| conns.contains(t)) {
        toks.remove(0);
    }
    while toks.last().is_some_and(|t| conns.contains(t)) {
        toks.pop();
    }
    toks.join(" ")
}

/// Compose a cardinal that the reverse table cannot hold on its own
/// (`> 10001`), e.g. "soixante-neuf mille huit" → 69008, "soixante-quinze mille
/// treize" → 75013. `None` if any fragment is not a known number word.
pub fn parse_scaled(lang: &str, text: &str) -> Option<i64> {
    if !supported_langs().contains(&lang) {
        return None;
    }
    parse_scaled_inner(lang, &normalize(text), &scale_words(lang), connector_words(lang))
}

fn parse_scaled_inner(lang: &str, text: &str, scales: &[(String, i64)], conns: &[&str]) -> Option<i64> {
    let text = trim_connectors(text.trim(), conns);
    let text = text.as_str();
    if text.is_empty() {
        return None;
    }
    // Fragment directly in the table (≤10001) — base case.
    if let Some(v) = lookup_plain(lang, text) {
        return Some(v);
    }
    // Split on the largest scale word present (whole-token match).
    let toks: Vec<&str> = text.split_whitespace().collect();
    for (word, mag) in scales {
        if let Some(pos) = toks.iter().position(|t| t == word) {
            let left = toks[..pos].join(" ");
            let right = toks[pos + 1..].join(" ");
            // « mille » nu (pas de multiplicateur à gauche) = 1×mille.
            let l = if trim_connectors(left.trim(), conns).is_empty() {
                1
            } else {
                parse_scaled_inner(lang, &left, scales, conns)?
            };
            let r = if trim_connectors(right.trim(), conns).is_empty() {
                0
            } else {
                parse_scaled_inner(lang, &right, scales, conns)?
            };
            return Some(l * mag + r);
        }
    }
    // Additive hundreds glued into one word ("novecento" = 900) followed by a
    // sub-hundred remainder ("ottantotto" = 88): 900 + 88 = 988. num2words
    // elides the vowel at the join ("novecentottantotto"), so the de-spaced
    // string is not the canonical spelling and only this additive split
    // recovers it. Fires under a scale too — "mille novecento ottantotto"
    // splits on "mille" then composes 900 + 88 here. Guarded so it never
    // fabricates: the left part must be a positive whole hundred, the right a
    // genuine sub-hundred, both real table words.
    let toks: Vec<&str> = text.split_whitespace().collect();
    for i in 1..toks.len() {
        let left = trim_connectors(&toks[..i].join(" "), conns);
        let right = trim_connectors(&toks[i..].join(" "), conns);
        if let (Some(l), Some(r)) = (lookup_plain(lang, &left), lookup_plain(lang, &right)) {
            if l >= 100 && l % 100 == 0 && (1..100).contains(&r) {
                return Some(l + r);
            }
        }
    }
    None
}

/// Port of `_rust.parse_int` — a plain `int(s)` for a signed ASCII integer.
pub fn parse_int(s: &str) -> Result<i64, std::num::ParseIntError> {
    s.parse::<i64>()
}

// ---------------------------------------------------------------------------
// Spoken "year" forms (below the LOOKUP_RANGE table but not canonical)
// ---------------------------------------------------------------------------
//
// num2words only ever renders ONE spelling per value, so the reverse table
// holds just that canonical form. But speech routinely uses the "year" reading
// — two 2-digit groups ("neunzehn neunundneunzig" = 19·100+99 = 1999), an
// explicit hundred ("dix-neuf cent quatre-vingt-dix" = 19·100+90 = 1990), or a
// hundred glued into one token ("nittonhundranittiosju" = 19·100+97 = 1997).
// None of those are the canonical render, so the table misses and the caller
// used to raise `cannot parse`. `parse_year` recovers them from the same
// reverse-table primitive, so it needs no per-language grammar.

/// Cache of the derived "hundred" morpheme per language (`None` when the
/// language has no regular one, e.g. es cien/-cientos).
fn hundred_cache() -> &'static RwLock<HashMap<String, Option<String>>> {
    static H: OnceLock<RwLock<HashMap<String, Option<String>>>> = OnceLock::new();
    H.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Longest common suffix of two ASCII-ish strings, by `char`.
fn common_suffix(a: &str, b: &str) -> String {
    let ac: Vec<char> = a.chars().collect();
    let bc: Vec<char> = b.chars().collect();
    let mut k = 0;
    while k < ac.len() && k < bc.len() && ac[ac.len() - 1 - k] == bc[bc.len() - 1 - k] {
        k += 1;
    }
    ac[ac.len() - k..].iter().collect()
}

/// Derive the language's "hundred" morpheme (fr "cent", de "hundert",
/// nl "honderd", sv "hundra", it "cento") by rendering 200/300/900, de-spacing,
/// dropping a trailing plural "s", and taking the longest common alphabetic
/// suffix (>=3 chars). Returns `None` for irregular hundreds (es cien/-cientos),
/// where `parse_year` simply skips the hundred split.
fn hundred_word(lang: &str) -> Option<String> {
    if let Some(v) = hundred_cache().read().unwrap().get(lang) {
        return v.clone();
    }
    let out = (|| {
        let l = num2words2_core::get_lang_by_key(lang)?;
        let mut forms: Vec<String> = Vec::new();
        // Probe every hundred 200..=900: the letter BEFORE the hundred morpheme
        // then varies across units (it quattr-o/se-i/nov-e, fr deu-x/si-x/hui-t),
        // so the longest common suffix isolates the morpheme itself ("cento",
        // "cent") rather than a unit's trailing vowel.
        for n in (200i64..=900).step_by(100) {
            let w = l.to_cardinal(&BigInt::from(n)).ok()?;
            let mut s: String = normalize(&w).split_whitespace().collect();
            if s.ends_with('s') {
                s.pop();
            }
            forms.push(s);
        }
        let mut suf = forms[0].clone();
        for f in &forms[1..] {
            suf = common_suffix(&suf, f);
        }
        // Keep only the trailing alphabetic run (drop any leading unit letters).
        let tail: String = {
            let mut rev: Vec<char> = suf.chars().rev().take_while(|c| c.is_alphabetic()).collect();
            rev.reverse();
            rev.into_iter().collect()
        };
        (tail.chars().count() >= 3).then_some(tail)
    })();
    hundred_cache()
        .write()
        .unwrap()
        .insert(lang.to_string(), out.clone());
    out
}

/// Recover a spoken "year" reading that is not num2words' canonical spelling:
/// `L <hundred> R` (explicit or glued) → `L*100 + R`, or two 2-digit groups
/// `a b` → `a*100 + b`. All parts are resolved through the reverse table, so no
/// language-specific grammar is needed. Intended as a LAST-resort fallback,
/// after the whole-string table hit and [`parse_scaled`] have both declined —
/// so a canonical number (`vingt trois` = 23) never reaches here.
pub fn parse_year(lang: &str, text: &str) -> Option<i64> {
    if !supported_langs().contains(&lang) {
        return None;
    }
    let norm = normalize(text);
    let toks: Vec<&str> = norm.split_whitespace().collect();
    if toks.is_empty() {
        return None;
    }
    let hw = hundred_word(lang);

    // (B) explicit hundred token: LEFT <hundred> RIGHT -> LEFT*100 + RIGHT.
    if let Some(ref h) = hw {
        if let Some(pos) = toks.iter().position(|t| *t == h.as_str()) {
            let left = toks[..pos].join(" ");
            let right = toks[pos + 1..].join(" ");
            let high = if left.is_empty() { Some(1) } else { lookup_plain(lang, &left) };
            let low = if right.is_empty() { Some(0) } else { lookup_plain(lang, &right) };
            if let (Some(h100), Some(l)) = (high, low) {
                if (1..=99).contains(&h100) && (0..=99).contains(&l) {
                    return Some(h100 * 100 + l);
                }
            }
        }
    }
    // (C) one glued token carrying the hundred morpheme: PRE<hundred>SUF.
    if toks.len() == 1 {
        if let Some(ref h) = hw {
            let t = toks[0];
            if let Some(pos) = t.find(h.as_str()) {
                let pre = &t[..pos];
                let suf = &t[pos + h.len()..];
                if !(pre.is_empty() && suf.is_empty()) {
                    let high = if pre.is_empty() { Some(1) } else { lookup_plain(lang, pre) };
                    let low = if suf.is_empty() { Some(0) } else { lookup_plain(lang, suf) };
                    if let (Some(h100), Some(l)) = (high, low) {
                        if (1..=99).contains(&h100) && (0..=99).contains(&l) {
                            return Some(h100 * 100 + l);
                        }
                    }
                }
            }
        }
    }
    // (A) two spoken 2-digit groups ("nineteen ninety-nine") -> a*100 + b.
    if toks.len() == 2 {
        if let (Some(a), Some(b)) = (lookup_plain(lang, toks[0]), lookup_plain(lang, toks[1])) {
            if (10..=99).contains(&a) && (0..=99).contains(&b) {
                return Some(a * 100 + b);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// English grammar entry points (pure)
// ---------------------------------------------------------------------------

/// `Words2Num_EN().to_cardinal(text)`.
pub fn en_to_cardinal(text: &str) -> Result<w2n_lang_en::W2nValue, w2n_lang_en::W2nError> {
    w2n_lang_en::W2nLangEn::new().to_cardinal(text)
}

/// `Words2Num_EN().to_ordinal(text)`.
pub fn en_to_ordinal(text: &str) -> Result<w2n_lang_en::W2nValue, w2n_lang_en::W2nError> {
    w2n_lang_en::W2nLangEn::new().to_ordinal(text)
}

/// `Words2Num_EN().to_year(text)`.
pub fn en_to_year(text: &str) -> Result<w2n_lang_en::W2nValue, w2n_lang_en::W2nError> {
    w2n_lang_en::W2nLangEn::new().to_year(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_python_shape() {
        // Hyphen between letters -> space; trailing punct -> space; collapse.
        assert_eq!(normalize("Forty-Two"), "forty two");
        assert_eq!(normalize("  a_b  "), "a b");
        assert_eq!(normalize("hello."), "hello");
        assert_eq!(normalize("3.14"), "3.14"); // decimal point survives
        assert_eq!(normalize("-17"), "-17"); // sign hyphen survives
        // NFKD + combining-mark strip: "trente-deux" family.
        assert_eq!(normalize("tr\u{e9}nte"), "trente"); // é -> e
        assert_eq!(normalize("f\u{f3}rty"), "forty"); // ó -> o
    }

    #[test]
    fn en_entry_points() {
        assert_eq!(
            en_to_cardinal("forty-two").unwrap(),
            w2n_lang_en::W2nValue::Int(BigInt::from(42))
        );
        assert_eq!(
            en_to_ordinal("twenty-first").unwrap(),
            w2n_lang_en::W2nValue::Int(BigInt::from(21))
        );
        assert_eq!(
            en_to_year("nineteen ninety nine").unwrap(),
            w2n_lang_en::W2nValue::Int(BigInt::from(1999))
        );
        assert!(en_to_cardinal("forty zoot").is_err());
    }

    #[test]
    fn scaled_cardinals_above_table() {
        // Reverse table only holds -1..10001; these compose via scale words.
        assert_eq!(parse_scaled("fr", "soixante-neuf mille huit"), Some(69008));
        assert_eq!(parse_scaled("fr", "cinquante-neuf mille"), Some(59000));
        assert_eq!(parse_scaled("fr", "soixante-quinze mille treize"), Some(75013));
        assert_eq!(parse_scaled("fr", "quatre-vingt-douze mille cent"), Some(92100));
        assert_eq!(parse_scaled("fr", "mille treize"), Some(1013));
        // Non-number fragment → None (walker keeps its shorter parse).
        assert_eq!(parse_scaled("fr", "mille lyon"), None);
        // Plural scale words, connectors, long-scale billions, Slavic genitive
        // plural — round-trips that the flat reverse table could never hold.
        assert_eq!(parse_scaled("fr", "un milliard"), Some(1_000_000_000));
        assert_eq!(parse_scaled("es", "sesenta y nueve mil ocho"), Some(69008));
        assert_eq!(parse_scaled("es", "dos millones"), Some(2_000_000)); // not 2e9
        assert_eq!(parse_scaled("es", "mil millones"), Some(1_000_000_000));
        assert_eq!(parse_scaled("pt", "sessenta e nove mil e oito"), Some(69008));
        assert_eq!(parse_scaled("pl", "sześćdziesiąt dziewięć tysięcy osiem"), Some(69008));
        assert_eq!(parse_scaled("ru", "шестьдесят девять тысяч восемь"), Some(69008));
        // Sentence walk emits the composed value inline.
        assert_eq!(
            w2n_sentence::words2num_sentence(
                "quarante-deux rue des freres lumiere soixante-neuf mille huit lyon",
                "fr",
                "cardinal",
                false,
            )
            .unwrap(),
            "42 rue des freres lumiere 69008 lyon"
        );
    }

    #[test]
    fn supported_langs_nonempty() {
        let langs = supported_langs();
        assert!(langs.contains(&"en"));
        assert!(langs.contains(&"fr"));
        assert!(langs.len() >= 100);
    }

    #[test]
    fn hundred_morpheme_is_derived() {
        assert_eq!(hundred_word("fr").as_deref(), Some("cent"));
        assert_eq!(hundred_word("it").as_deref(), Some("cento"));
        assert_eq!(hundred_word("de").as_deref(), Some("hundert"));
        assert_eq!(hundred_word("nl").as_deref(), Some("honderd"));
        assert_eq!(hundred_word("sv").as_deref(), Some("hundra"));
    }

    #[test]
    fn spoken_year_forms() {
        // Two 2-digit groups ("nineteen ninety-nine").
        assert_eq!(parse_year("de", "neunzehn neunundneunzig"), Some(1999));
        assert_eq!(parse_year("nl", "negentien zevenennegentig"), Some(1997));
        // Explicit hundred, spaced (fr "dix-neuf cent ...", hyphens → spaces).
        assert_eq!(parse_year("fr", "dix neuf cent quatre vingt dix"), Some(1990));
        assert_eq!(parse_year("fr", "dix-neuf cent quatre-vingt-dix"), Some(1990));
        // Hundred glued into one token.
        assert_eq!(parse_year("sv", "nittonhundranittiosju"), Some(1997));
        assert_eq!(parse_year("nl", "negentienhonderdzevenennegentig"), Some(1997));
        // A plain canonical two-token number must NOT be mis-read as a year:
        // "vingt trois" (23) is a table hit, so the fallback never sees it —
        // but even directly, 20·100+3 is refused because 23 resolves first via
        // the caller; here we assert the guard shape holds for a non-century.
        assert_eq!(parse_year("fr", "trois quatre"), None); // 3,4 not in 10..99
    }

    #[test]
    fn year_forms_via_public_entry() {
        use w2n_lang_en::W2nValue;
        use w2n_sentence::words2num;
        let int = |n: i64| W2nValue::Int(BigInt::from(n));
        // End-to-end through the same path Python's `words2num` takes.
        assert_eq!(words2num("neunzehn neunundneunzig", "de", "cardinal").unwrap(), int(1999));
        assert_eq!(words2num("nittonhundranittiosju", "sv", "cardinal").unwrap(), int(1997));
        // De-spaced glued canonical ("mille novecento ottantotto" → 1988).
        assert_eq!(words2num("mille novecento ottantotto", "it", "cardinal").unwrap(), int(1988));
        // Regression: a canonical number is unaffected.
        assert_eq!(words2num("deux mille dix", "fr", "cardinal").unwrap(), int(2010));
        assert_eq!(words2num("vingt trois", "fr", "cardinal").unwrap(), int(23));
    }
}
