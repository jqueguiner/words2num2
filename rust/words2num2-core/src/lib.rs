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

/// Scale words for `lang`, biggest magnitude first. Derived by rendering
/// 10^9/10^6/10^3 via num2words and taking the last token of each (fr → "milliard",
/// "million", "mille"). A magnitude whose render is a single glued token that
/// equals a smaller one, or that collides, is skipped.
fn scale_words(lang: &str) -> Vec<(String, i64)> {
    if let Some(v) = scale_cache().read().unwrap().get(lang) {
        return v.clone();
    }
    let mut out: Vec<(String, i64)> = Vec::new();
    if let Some(l) = num2words2_core::get_lang_by_key(lang) {
        for mag in [1_000_000_000i64, 1_000_000, 1_000] {
            if let Ok(words) = l.to_cardinal(&BigInt::from(mag)) {
                let norm = normalize(&words);
                if let Some(last) = norm.split_whitespace().last() {
                    let w = last.to_string();
                    // Le mot d'échelle doit être un token DISTINCT (pas déjà pris,
                    // ≥3 lettres) — évite les faux positifs sur les langues collées.
                    if w.len() >= 3 && !out.iter().any(|(x, _)| *x == w) {
                        out.push((w, mag));
                    }
                }
            }
        }
    }
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

/// Compose a cardinal that the reverse table cannot hold on its own
/// (`> 10001`), e.g. "soixante-neuf mille huit" → 69008, "soixante-quinze mille
/// treize" → 75013. `None` if any fragment is not a known number word.
pub fn parse_scaled(lang: &str, text: &str) -> Option<i64> {
    if !supported_langs().contains(&lang) {
        return None;
    }
    parse_scaled_inner(lang, &normalize(text), &scale_words(lang))
}

fn parse_scaled_inner(lang: &str, text: &str, scales: &[(String, i64)]) -> Option<i64> {
    let text = text.trim();
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
            let l = if left.trim().is_empty() {
                1
            } else {
                parse_scaled_inner(lang, &left, scales)?
            };
            let r = if right.trim().is_empty() {
                0
            } else {
                parse_scaled_inner(lang, &right, scales)?
            };
            return Some(l * mag + r);
        }
    }
    None
}

/// Port of `_rust.parse_int` — a plain `int(s)` for a signed ASCII integer.
pub fn parse_int(s: &str) -> Result<i64, std::num::ParseIntError> {
    s.parse::<i64>()
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
}
