//! Port of the words2num2 **sentence-level API**.
//!
//! Sources (the specification — bugs included):
//! - `words2num2/__init__.py`: `_resolve_lang`, `words2num_sentence`, `convert_sentence`, `sentence_to_words`
//! - `words2num2/converters/sentence.py`: `SentenceConverter`
//! - `words2num2/converters/auto.py`: `UNITS`, `CURRENCIES`, `Quantity`, `auto_parse`, `auto_parse_sentence`
//!
//! Every per-language `Words2Num_*` converter is dispatched **in Rust** by the
//! [`Converter`] abstraction below: `en` uses the hand-written grammar
//! ([`crate::w2n_lang_en`]); every other locale uses the generic reverse-table
//! lookup ([`crate::lookup`]) plus the `_parse_literal` tail — exactly what
//! `Words2Num_Base` does in Python, but without leaving Rust.
//!
//! # Fidelity notes — behaviour reproduced on purpose
//!
//! Verified against the live interpreter. These all look wrong and are all
//! correct ports:
//!
//! * `words2num_sentence("nineteen ninety nine")` → `"118"`, **not** `"1999"`.
//!   The sentence walker calls `to_cardinal` (19 + 99), never `to_year`.
//! * `words2num_sentence("minus forty two")` → `"minus 42"`. A run may not
//!   *start* with a connector, and `to_cardinal("minus")` raises, so "minus"
//!   is not a run head. Same for `"a hundred and one dogs"` → `"a 101 dogs"`.
//! * `words2num_sentence("point five")` → `"0.5"`. `to_cardinal("point")`
//!   returns `Decimal(0)` rather than raising, so "point" *is* a valid head.
//! * `words2num_sentence("\"forty-two\"")` → `"42\""`. Only *trailing*
//!   punctuation is stripped, and the trailing quote is re-appended.
//! * `words2num_sentence("zero point zero zero zero zero zero zero one")`
//!   → `"1E-7"` — Python's `Decimal.__str__` flips to scientific notation
//!   once the adjusted exponent drops below -6. See [`py_decimal_str`].
//! * `auto_parse("$5kn")` raises **KeyError**, not `Words2NumError`: the
//!   regex accepts `[kKmMbBtT][nN]?` but `SCALE_SUFFIXES` only holds
//!   `k K m M b B bn t T tn`. It escapes `auto_parse_sentence` uncaught
//!   because `_replace` only catches `Words2NumError`. See [`W2nError::Key`].
//! * `parse_number_string("0.5", lang="fr")` → `5`. French decimal is `,`,
//!   so the dot is dropped as a stray separator and `"05"` parses as an int.
//! * `_try_word_unit`'s `long_name = info.long` assignment is dead — it is
//!   unconditionally overwritten by the `next(...)` scan below it. Not ported.
//! * `word_units["pound sterling"]` is unreachable: the key holds a space but
//!   the lookup token comes from `rsplit(None, 1)`, so it never contains one.
//! * The `°[CF]?` alternative appears **twice** in `_try_digit_unit`'s regex.
//!   Harmless; the second is dead. Kept in the comment, folded in the code.

use bigdecimal::num_traits::FromPrimitive;
use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use std::collections::HashMap;
use std::str::FromStr;

// ===========================================================================
// Errors
// ===========================================================================

/// The exception kinds this layer can produce, each carrying the *exact*
/// message Python formats. The PyO3 binder maps each variant onto the matching
/// Python exception class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum W2nError {
    /// `words2num2.base.Words2NumError` — a `ValueError` subclass.
    Words2Num(String),
    /// `NotImplementedError` — raised by `_resolve_lang`.
    NotImplemented(String),
    /// `KeyError` — `SCALE_SUFFIXES[scale_str]` misses (e.g. `"$5kn"`).
    /// Python's `KeyError` stringifies as `repr(key)`, hence the odd quoting.
    Key(String),
}

impl W2nError {
    /// The message text Python would carry (`str(exc)`).
    ///
    /// Note `KeyError` is special-cased: `str(KeyError("kn"))` is `"'kn'"`,
    /// not `"kn"`.
    pub fn message(&self) -> String {
        match self {
            W2nError::Words2Num(m) | W2nError::NotImplemented(m) => m.clone(),
            W2nError::Key(k) => py_repr_str(k),
        }
    }
}

// ===========================================================================
// Values — words2num2 returns int | float | Decimal
// ===========================================================================

/// A value as words2num2 produces it.
///
/// `Dec` mirrors Python's `decimal.Decimal` as the (coefficient, exponent)
/// pair that `Decimal.as_tuple()` exposes: `BigDecimal`'s
/// `as_bigint_and_exponent()` returns `(digits, scale)` where the value is
/// `digits * 10^-scale`, i.e. `_int = digits.abs()` and `_exp = -scale`.
/// Keeping the *unnormalised* scale is load-bearing — `Decimal("2.0")` and
/// `Decimal("2")` are equal but stringify differently.
#[derive(Debug, Clone, PartialEq)]
pub enum W2nValue {
    Int(BigInt),
    Float(f64),
    Dec(BigDecimal),
}

impl W2nValue {
    /// Python's `str(value)`.
    pub fn py_str(&self) -> String {
        match self {
            W2nValue::Int(i) => i.to_string(),
            W2nValue::Float(f) => py_float_repr(*f),
            W2nValue::Dec(d) => {
                let (digits, scale) = d.as_bigint_and_exponent();
                py_decimal_str(&digits, scale)
            }
        }
    }

    /// Python's `repr(value)`. Differs from `str` only for `Decimal`.
    pub fn py_repr(&self) -> String {
        match self {
            W2nValue::Dec(_) => format!("Decimal('{}')", self.py_str()),
            _ => self.py_str(),
        }
    }

    /// `value == 1 or value == -1`, as Python's `value in (1, -1, 1.0, -1.0)`
    /// evaluates it (membership uses `==`, so `Decimal("1")` matches).
    fn is_unit_magnitude(&self) -> bool {
        match self {
            W2nValue::Int(i) => {
                let one = BigInt::from(1);
                *i == one || *i == -one
            }
            W2nValue::Float(f) => *f == 1.0 || *f == -1.0,
            W2nValue::Dec(d) => {
                let one = BigDecimal::from(1);
                *d == one || *d == -one
            }
        }
    }
}

/// Convert an English-grammar value ([`crate::w2n_lang_en::W2nValue`]) into a
/// sentence-layer [`W2nValue`].
///
/// LIMITATION: `PyDec` carries a signed zero (`Decimal('-0.0')`), which
/// `BigDecimal` cannot; a negative-zero decimal therefore loses its sign here.
/// It is unreachable through the sentence walker (a run cannot start with
/// "minus", so a negative decimal never heads a run) and through the tested
/// `auto_parse` paths.
fn en_to_sentence_value(v: crate::w2n_lang_en::W2nValue) -> W2nValue {
    use crate::w2n_lang_en::W2nValue as E;
    match v {
        E::Int(i) => W2nValue::Int(i),
        E::Float(f) => W2nValue::Float(f),
        E::Dec(d) => W2nValue::Dec(d.to_bigdecimal()),
    }
}

// ===========================================================================
// Python string / number formatting primitives
// ===========================================================================

/// Python's `repr()` for `str`, used by every `%r` in the source.
///
/// Quote selection matches CPython: single quotes unless the string contains
/// a single quote and no double quote.
///
/// LIMITATION: CPython escapes non-printable *non-ASCII* (per the unicodedata
/// category) as `\xXX` / `\uXXXX` / `\UXXXXXXXX`. Here non-ASCII passes
/// through verbatim. Every `%r` reachable in this module formats a locale code,
/// a unit token or a type name, so the gap is unreachable in practice.
pub fn py_repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Python's `repr()`/`str()` for `float` (they are the same in Python 3).
///
/// CPython's `format_float_short(..., 'r', ...)`: take the shortest digit
/// string that round-trips, then switch to exponential iff
/// `decpt <= -4 || decpt > 16`, where `decpt` is the decimal point position
/// (`value = 0.<digits> * 10^decpt`). `Py_DTSF_ADD_DOT_0` appends `.0` in the
/// fixed branch only.
///
/// Rust's `{:e}` already yields the shortest round-tripping digits, so it is
/// used purely as a digit/exponent source and then reformatted Python-style.
/// This matters: Rust's own `{}` would print `1e16` as `10000000000000000`
/// and `1e-5` as `0.00001`, where Python prints `1e+16` and `1e-05`.
pub fn py_float_repr(v: f64) -> String {
    if v.is_nan() {
        return "nan".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    let sci = format!("{:e}", v); // e.g. "-1.2345e3", "0e0", "1e-7"
    let (mant, exp) = sci.split_once('e').expect("{:e} always emits an exponent");
    let exp: i32 = exp.parse().expect("{:e} exponent is an integer");
    let neg = mant.starts_with('-');
    let mant = mant.trim_start_matches('-');
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    let decpt = exp + 1;
    let sign = if neg { "-" } else { "" };
    let nd = digits.len() as i32;

    if decpt <= -4 || decpt > 16 {
        // Exponential: d[.ddd]e{+,-}XX, exponent at least two digits.
        let mut m = String::from(&digits[..1]);
        if nd > 1 {
            m.push('.');
            m.push_str(&digits[1..]);
        }
        let e = decpt - 1;
        let (esign, eabs) = if e < 0 { ("-", -e) } else { ("+", e) };
        return format!("{}{}e{}{:02}", sign, m, esign, eabs);
    }
    if decpt <= 0 {
        format!("{}0.{}{}", sign, "0".repeat((-decpt) as usize), digits)
    } else if decpt >= nd {
        // ADD_DOT_0
        format!(
            "{}{}{}.0",
            sign,
            digits,
            "0".repeat((decpt - nd) as usize)
        )
    } else {
        format!(
            "{}{}.{}",
            sign,
            &digits[..decpt as usize],
            &digits[decpt as usize..]
        )
    }
}

/// Python's `Decimal.__str__` (from `_pydecimal.__str__`, `eng=False`,
/// `context.capitals = 1`), reconstructed from the coefficient and exponent:
///
/// ```text
/// leftdigits = _exp + len(_int)
/// if _exp <= 0 and leftdigits > -6:  dotplace = leftdigits   # plain
/// else:                              dotplace = 1            # scientific
/// ```
///
/// This is what turns `Decimal("0.0000001")` into `"1E-7"`.
///
/// `digits`/`scale` come from `BigDecimal::as_bigint_and_exponent()`:
/// value = `digits * 10^-scale`, so `_exp = -scale` and `_int = |digits|`.
pub fn py_decimal_str(digits: &BigInt, scale: i64) -> String {
    let neg = digits.sign() == num_bigint::Sign::Minus;
    let int_str = digits.magnitude().to_string(); // _int, unsigned
    let exp: i64 = -scale; // _exp
    let len_int = int_str.chars().count() as i64;
    let leftdigits = exp + len_int;

    let dotplace = if exp <= 0 && leftdigits > -6 {
        leftdigits
    } else {
        1
    };

    let (intpart, fracpart) = if dotplace <= 0 {
        (
            String::from("0"),
            format!(".{}{}", "0".repeat((-dotplace) as usize), int_str),
        )
    } else if dotplace >= len_int {
        (
            format!("{}{}", int_str, "0".repeat((dotplace - len_int) as usize)),
            String::new(),
        )
    } else {
        let cut = dotplace as usize;
        (int_str[..cut].to_string(), format!(".{}", &int_str[cut..]))
    };

    let expo = if leftdigits == dotplace {
        String::new()
    } else {
        format!("E{:+}", leftdigits - dotplace)
    };
    format!(
        "{}{}{}{}",
        if neg { "-" } else { "" },
        intpart,
        fracpart,
        expo
    )
}

// ===========================================================================
// Unicode / Python text primitives
// ===========================================================================

/// Starts of every 10-codepoint run in Unicode category `Nd`, which is exactly
/// what Python's `\d` matches for `str` patterns (and what `int()` / `float()`
/// accept). Every `Nd` block is a contiguous ten with digit value 0 at the
/// start, so a block start plus an offset is all that is needed.
///
/// Generated from the local interpreter (`unicodedata.unidata_version` 13.0.0,
/// CPython 3.9). A newer CPython knows a few more blocks; regenerate if the
/// oracle moves. Without this, `auto_parse("٤٢")` would fail where Python
/// returns `42`.
const ND_BLOCKS: [u32; 65] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0,
    0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x16A60, 0x16B50,
    0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E950, 0x1FBF0,
];

/// The digit value of `c` if it is in category `Nd`, else `None`.
pub fn digit_value(c: char) -> Option<u32> {
    let cp = c as u32;
    match ND_BLOCKS.binary_search(&cp) {
        Ok(_) => Some(0),
        Err(0) => None,
        Err(i) => {
            let start = ND_BLOCKS[i - 1];
            if cp - start < 10 {
                Some(cp - start)
            } else {
                None
            }
        }
    }
}

/// Python's regex `\d` for `str` patterns.
pub fn is_unicode_digit(c: char) -> bool {
    digit_value(c).is_some()
}

/// Fold every `Nd` digit down to ASCII, which is what `int()` / `float()` do
/// internally before parsing.
fn to_ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| match digit_value(c) {
            Some(v) if !c.is_ascii_digit() => char::from_digit(v, 10).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Python's `str.isspace()` per character — equivalently the regex `\s` for
/// `str` patterns. Rust's `char::is_whitespace` agrees except for the four
/// C0 separators `\x1c-\x1f`, which Python counts and Rust does not.
pub fn py_is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python's `str.isspace()` — all characters are space, and the string is
/// non-empty.
fn py_str_isspace(s: &str) -> bool {
    !s.is_empty() && s.chars().all(py_is_space)
}

/// Python's `str.strip()` (no argument).
fn py_strip(s: &str) -> &str {
    s.trim_matches(py_is_space)
}

/// Python's `str.split()` (no argument): split on runs of whitespace,
/// discarding empty fields.
fn py_split_whitespace(s: &str) -> Vec<&str> {
    s.split(py_is_space).filter(|p| !p.is_empty()).collect()
}

/// Python's `str.rsplit(None, 1)`.
///
/// Trailing whitespace is skipped, the last whitespace-free run becomes the
/// tail, and the head keeps *its own* leading whitespace but loses the
/// separator run — `"  a  b  ".rsplit(None, 1) == ["  a", "b"]`.
fn py_rsplit_once_ws(s: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut end = chars.len();
    while end > 0 && py_is_space(chars[end - 1].1) {
        end -= 1;
    }
    if end == 0 {
        return Vec::new();
    }
    let mut tail_start = end;
    while tail_start > 0 && !py_is_space(chars[tail_start - 1].1) {
        tail_start -= 1;
    }
    let byte_end = if end == chars.len() {
        s.len()
    } else {
        chars[end].0
    };
    let tail = &s[chars[tail_start].0..byte_end];
    let mut head_end = tail_start;
    while head_end > 0 && py_is_space(chars[head_end - 1].1) {
        head_end -= 1;
    }
    if head_end == 0 {
        return vec![tail];
    }
    vec![&s[..chars[head_end].0], tail]
}

/// Python's `str.rstrip(chars)` for a single-character set.
fn py_rstrip_char(s: &str, ch: char) -> &str {
    s.trim_end_matches(ch)
}

/// `re.sub(r"[\.,;:!\?\"']+$", "", s)` — drop the trailing punctuation run.
fn rstrip_punct(s: &str) -> &str {
    s.trim_end_matches(['.', ',', ';', ':', '!', '?', '"', '\''])
}

/// `re.search(r"[\.,;:!\?\"']+$", s)` → `m.group()`, or `""`.
///
/// The leftmost match of `[...]+$` is exactly the maximal trailing run.
fn trailing_punct(s: &str) -> &str {
    let kept = rstrip_punct(s);
    &s[kept.len()..]
}

/// `re.search(r"[\.,;:!\?]$", tok)` — note this class has **no** quote
/// characters, unlike the strip above: `forty-two"` does not close a run.
fn ends_with_terminal_punct(s: &str) -> bool {
    matches!(
        s.chars().next_back(),
        Some('.') | Some(',') | Some(';') | Some(':') | Some('!') | Some('?')
    )
}

/// `re.findall(r"\S+|\s+", sentence)` — alternating runs of non-space and
/// space. The two branches are complementary, so this is a simple run split
/// and `parts.concat() == sentence` always holds.
fn tokenize(sentence: &str) -> Vec<String> {
    let chars: Vec<char> = sentence.chars().collect();
    let mut parts = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let space = py_is_space(chars[i]);
        let start = i;
        while i < chars.len() && py_is_space(chars[i]) == space {
            i += 1;
        }
        parts.push(chars[start..i].iter().collect());
    }
    parts
}

// ===========================================================================
// `_resolve_lang`
// ===========================================================================

/// The keys of `CONVERTER_CLASSES` in `words2num2/__init__.py`, in source
/// order. Membership in *this* set is what `_resolve_lang` tests, which is
/// **not** the same set as `num2words2_core::supported_lang_keys()` — the
/// aliases `jp` and `cn` live only here.
///
/// Must stay in sync with `__init__.py`.
pub const CONVERTER_LANGS: [&str; 120] = [
    "af", "am", "ar", "as", "az", "ba", "be", "bg", "bn", "bo", "br", "bs", "ca", "ce", "cs", "cy",
    "da", "de", "el", "en", "en_IN", "en_NG", "eo", "es", "es_CO", "es_CR", "es_GT", "es_NI",
    "es_VE", "et", "eu", "fa", "fi", "fo", "fr", "fr_BE", "fr_CH", "fr_DZ", "gl", "gu", "ha", "haw",
    "he", "hi", "hr", "ht", "hu", "hy", "id", "is", "it", "ja", "jw", "ka", "kk", "km", "kn", "ko",
    "kz", "la", "lb", "ln", "lo", "lt", "lv", "mg", "mi", "mk", "ml", "mn", "mr", "ms", "mt", "my",
    "ne", "nl", "nn", "no", "oc", "pa", "pl", "ps", "pt", "pt_BR", "ro", "ru", "sa", "sd", "si",
    "sk", "sl", "sn", "so", "sq", "sr", "su", "sv", "sw", "ta", "te", "tet", "tg", "th", "tk", "tl",
    "tr", "tt", "uk", "ur", "uz", "vi", "wo", "yi", "yo", "zh", "zh_CN", "zh_HK", "zh_TW", "jp",
    "cn",
];

fn is_known_lang(k: &str) -> bool {
    CONVERTER_LANGS.contains(&k)
}

/// Port of `words2num2._resolve_lang`.
///
/// ```python
/// if lang in CONVERTER_CLASSES: return lang
/// normalized = lang.replace("-", "_")
/// if normalized in CONVERTER_CLASSES: return normalized
/// parts = normalized.split("_")
/// if len(parts) >= 2:
///     candidate = "{}_{}".format(parts[0].lower(), parts[1].upper())
///     if candidate in CONVERTER_CLASSES: return candidate
///     if parts[0] in CONVERTER_CLASSES: return parts[0]
/// if normalized[:2] in CONVERTER_CLASSES: return normalized[:2]
/// raise NotImplementedError("language %r is not supported" % lang)
/// ```
///
/// `normalized[:2]` is a *character* slice and never panics on a short string
/// — `_resolve_lang("e")` falls through to the raise, it does not blow up.
/// That last rule is why `"eng"` resolves to `"en"` and `"en_US"` to `"en"`.
pub fn resolve_lang(lang: &str) -> Result<String, W2nError> {
    if is_known_lang(lang) {
        return Ok(lang.to_string());
    }
    let normalized = lang.replace('-', "_");
    if is_known_lang(&normalized) {
        return Ok(normalized);
    }
    // Python's str.split("_") keeps empty fields, unlike split(None).
    let parts: Vec<&str> = normalized.split('_').collect();
    if parts.len() >= 2 {
        let candidate = format!("{}_{}", parts[0].to_lowercase(), parts[1].to_uppercase());
        if is_known_lang(&candidate) {
            return Ok(candidate);
        }
        if is_known_lang(parts[0]) {
            return Ok(parts[0].to_string());
        }
    }
    let two: String = normalized.chars().take(2).collect();
    if is_known_lang(&two) {
        return Ok(two);
    }
    Err(W2nError::NotImplemented(format!(
        "language {} is not supported",
        py_repr_str(lang)
    )))
}

// ===========================================================================
// Converter — the per-locale dispatch, entirely in Rust
// ===========================================================================

/// `Words2Num_Base.NEGATIVE_WORDS` — every generic locale inherits this; no
/// locale module overrides it.
const BASE_NEGATIVE_WORDS: [&str; 2] = ["minus", "negative"];

/// A per-locale converter, standing in for `CONVERTER_CLASSES[lang]`.
enum Converter {
    /// `Words2Num_EN` — the one hand-written grammar.
    En,
    /// A `Words2Num_Base` subclass; the field is the num2words2 core key its
    /// `LANG` attribute carries (e.g. `"fr"`, `"zh_CN"`).
    Table(&'static str),
}

/// Resolve a `CONVERTER_CLASSES` key (as produced by [`resolve_lang`]) to its
/// [`Converter`].
///
/// Most keys map to the same `LANG`, but three do not — matching the Python
/// registry: `"zh"`/`"cn"` both use `Words2Num_ZH_CN` (`LANG="zh_CN"`) and
/// `"jp"` uses `Words2Num_JA` (`LANG="ja"`).
fn converter_for(resolved: &str) -> Converter {
    match resolved {
        "en" => Converter::En,
        "zh" | "cn" => Converter::Table("zh_CN"),
        "jp" => Converter::Table("ja"),
        // `resolved` is guaranteed to be one of CONVERTER_LANGS.
        other => {
            let key = CONVERTER_LANGS
                .iter()
                .copied()
                .find(|k| *k == other)
                .unwrap_or("en");
            Converter::Table(key)
        }
    }
}

impl Converter {
    /// `converter.to_cardinal(token)` did not raise? (`_token_is_number_word`).
    fn is_number_word(&self, token: &str) -> bool {
        self.to_cardinal(token).is_ok()
    }

    fn to_cardinal(&self, text: &str) -> Result<W2nValue, W2nError> {
        match self {
            Converter::En => en_convert(crate::en_to_cardinal(text)),
            Converter::Table(lang) => base_convert(lang, text, false),
        }
    }

    fn to_ordinal(&self, text: &str) -> Result<W2nValue, W2nError> {
        match self {
            Converter::En => en_convert(crate::en_to_ordinal(text)),
            Converter::Table(lang) => base_convert(lang, text, true),
        }
    }

    fn to_year(&self, text: &str) -> Result<W2nValue, W2nError> {
        match self {
            Converter::En => en_convert(crate::en_to_year(text)),
            // `Words2Num_Base.to_year` == `self.to_cardinal`.
            Converter::Table(lang) => base_convert(lang, text, false),
        }
    }

    /// `Words2Num_Base.to_ordinal_num` — EN inherits it unchanged.
    fn to_ordinal_num(&self, text: &str) -> Result<W2nValue, W2nError> {
        base_ordinal_num(text)
    }

    /// `Words2Num_Base.to_currency` == `self.to_cardinal` (polymorphic, so EN
    /// currency uses EN cardinal).
    fn to_currency(&self, text: &str) -> Result<W2nValue, W2nError> {
        self.to_cardinal(text)
    }

    /// Digit-sequence mode: spoken digit-by-digit spellings ("two seven five"
    /// → 275, "sieben acht" → 78) are concatenated, not summed. The cardinal
    /// grammar mis-parses these (e.g. "two seven five" → 14); dictated house
    /// numbers, zip codes and phone-style runs are commonly spelled this way.
    ///
    /// Every whitespace token of the run must be a single-digit number word
    /// (0-9) in the target language; any non-digit (or 10+) makes this decline
    /// with `Err`, so the sentence walker falls back to its longest cardinal
    /// parse and never turns a real cardinal into a digit string.
    ///
    /// Limitation: the sentence value type is numeric, so a leading zero
    /// ("zero one" → 01) collapses to its integer (1). Runs whose concatenation
    /// overflows `i64` also decline (left as words).
    fn to_digits(&self, text: &str) -> Result<W2nValue, W2nError> {
        let dehyphened = text.replace('-', " ");
        let toks = py_split_whitespace(&dehyphened);
        if toks.is_empty() {
            return Err(W2nError::Words2Num("empty digit run".to_string()));
        }
        let mut s = String::with_capacity(toks.len());
        for t in &toks {
            match self.to_cardinal(t)? {
                // A single decimal digit stringifies to exactly one ASCII digit
                // (0-9); anything else (negatives, 10+, floats) declines.
                W2nValue::Int(d) => {
                    let ds = d.to_string();
                    let b = ds.as_bytes();
                    if b.len() == 1 && b[0].is_ascii_digit() {
                        s.push(b[0] as char);
                    } else {
                        return Err(W2nError::Words2Num("not a single digit".to_string()));
                    }
                }
                _ => return Err(W2nError::Words2Num("not a single digit".to_string())),
            }
        }
        // BigInt parse — no overflow. (Leading zeros collapse numerically.)
        let n: BigInt = s
            .parse()
            .map_err(|_| W2nError::Words2Num("bad digit run".to_string()))?;
        Ok(W2nValue::Int(n))
    }

    /// Dispatch `getattr(converter, "to_{to}")(token, **kwargs)`.
    ///
    /// `has_kwargs` models the sentence walker passing `**kwargs` through: the
    /// standard converters accept none, so Python raises `TypeError` there,
    /// which the walker swallows as "not a number word". An unknown `to`
    /// raises `AttributeError` in Python (`getattr` miss) — also swallowed.
    /// Both are represented here as an `Err` the caller drops.
    fn convert(&self, to: &str, text: &str, has_kwargs: bool) -> Result<W2nValue, W2nError> {
        if has_kwargs {
            // TypeError: to_*() takes no keyword arguments.
            return Err(W2nError::Words2Num(
                "unexpected keyword argument".to_string(),
            ));
        }
        match to {
            "cardinal" => self.to_cardinal(text),
            "ordinal" => self.to_ordinal(text),
            "year" => self.to_year(text),
            "ordinal_num" => self.to_ordinal_num(text),
            "currency" => self.to_currency(text),
            "digits" => self.to_digits(text),
            // AttributeError: converter has no to_<to>.
            _ => Err(W2nError::NotImplemented(format!(
                "'Words2Num' object has no attribute 'to_{}'",
                to
            ))),
        }
    }
}

/// Wrap the English grammar's `Result` into the sentence-layer types.
fn en_convert(
    r: Result<crate::w2n_lang_en::W2nValue, crate::w2n_lang_en::W2nError>,
) -> Result<W2nValue, W2nError> {
    match r {
        Ok(v) => Ok(en_to_sentence_value(v)),
        Err(e) => Err(W2nError::Words2Num(e.msg)),
    }
}

/// Port of `Words2Num_Base._convert`: reverse-table lookup, then the
/// sign/digit/error tail (`_parse_literal`).
fn base_convert(lang: &str, text: &str, ordinal: bool) -> Result<W2nValue, W2nError> {
    // `_rust_lookup`: guarded on `LANG in _RUST_LANGS`, and any error from the
    // core is swallowed to `None` (`except Exception: return None`).
    if crate::supported_langs().contains(&lang) {
        let neg = [
            BASE_NEGATIVE_WORDS[0].to_string(),
            BASE_NEGATIVE_WORDS[1].to_string(),
        ];
        if let Ok(Some(v)) = crate::lookup(lang, text, ordinal, &neg) {
            return Ok(W2nValue::Int(BigInt::from(v)));
        }
        // Composition multi-échelle pour les valeurs hors table (> 10001) :
        // « soixante-neuf mille huit » → 69008. Uniquement en cardinal.
        if !ordinal {
            if let Some(v) = crate::parse_scaled(lang, text) {
                return Ok(W2nValue::Int(BigInt::from(v)));
            }
            // Compound spoken as separate tokens where num2words renders it
            // glued: "mille novecento ottantotto" → the canonical
            // "millenovecentottantotto". Only when de-spacing actually changes
            // the string (multi-token input), and only if that glued form is a
            // genuine table hit — so it never invents a value.
            let norm = crate::normalize(text);
            let despaced: String = norm.split_whitespace().collect();
            if despaced != norm {
                if let Ok(Some(v)) = crate::lookup(lang, &despaced, false, &neg) {
                    return Ok(W2nValue::Int(BigInt::from(v)));
                }
            }
            // Spoken "year" readings (two 2-digit groups, or an explicit/glued
            // hundred) that are not num2words' canonical spelling.
            if let Some(v) = crate::parse_year(lang, text) {
                return Ok(W2nValue::Int(BigInt::from(v)));
            }
        }
    }
    parse_literal(text)
}

/// Port of `Words2Num_Base._parse_literal` — a bare digit string, a leading
/// sign word, or genuinely unparseable input.
///
/// `errmsg_unparseable` is `"cannot parse %r as a number"`.
fn parse_literal(text: &str) -> Result<W2nValue, W2nError> {
    let normalized = crate::normalize(text);
    let unparseable = |s: &str| W2nError::Words2Num(format!("cannot parse {} as a number", py_repr_str(s)));
    if normalized.is_empty() {
        return Err(unparseable(&normalized));
    }
    for neg in BASE_NEGATIVE_WORDS {
        if let Some(rest) = normalized.strip_prefix(&format!("{} ", neg)) {
            return finish_parse_literal(rest, -1);
        }
        if normalized == neg {
            return Err(unparseable(&normalized));
        }
    }
    finish_parse_literal(&normalized, 1)
}

/// The `try: … except ValueError: pass; raise` tail of `_parse_literal`.
fn finish_parse_literal(normalized: &str, sign: i64) -> Result<W2nValue, W2nError> {
    if normalized.contains('.') {
        if let Some(f) = py_float(normalized) {
            return Ok(W2nValue::Float(if sign < 0 { -f } else { f }));
        }
    } else if let Some(i) = py_int(normalized) {
        return Ok(W2nValue::Int(if sign < 0 { -i } else { i }));
    }
    Err(W2nError::Words2Num(format!(
        "cannot parse {} as a number",
        py_repr_str(normalized)
    )))
}

/// Port of `Words2Num_Base.to_ordinal_num`.
///
/// ```python
/// m = re.search(r"-?\d+", text)
/// if not m: raise Words2NumError("cannot parse %r as a number" % text)
/// return int(m.group())
/// ```
fn base_ordinal_num(text: &str) -> Result<W2nValue, W2nError> {
    let chars: Vec<char> = text.chars().collect();
    for i in 0..chars.len() {
        // `-?\d+`: a '-' counts only when a digit follows it.
        if chars[i] == '-' && chars.get(i + 1).copied().is_some_and(is_unicode_digit) {
            let mut j = i + 1;
            while j < chars.len() && is_unicode_digit(chars[j]) {
                j += 1;
            }
            let group: String = chars[i..j].iter().collect();
            return py_int(&group)
                .map(W2nValue::Int)
                .ok_or_else(|| W2nError::Words2Num(unparseable_msg(text)));
        }
        if is_unicode_digit(chars[i]) {
            let mut j = i;
            while j < chars.len() && is_unicode_digit(chars[j]) {
                j += 1;
            }
            let group: String = chars[i..j].iter().collect();
            return py_int(&group)
                .map(W2nValue::Int)
                .ok_or_else(|| W2nError::Words2Num(unparseable_msg(text)));
        }
    }
    Err(W2nError::Words2Num(unparseable_msg(text)))
}

fn unparseable_msg(text: &str) -> String {
    format!("cannot parse {} as a number", py_repr_str(text))
}

/// Python's `int(str)` over a `_normalize`-shaped string (optional sign, then
/// Unicode `Nd` digits). Returns `None` where Python raises `ValueError`.
fn py_int(s: &str) -> Option<BigInt> {
    let t = s.trim_matches(py_is_space);
    let (neg, body) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if body.is_empty() || !body.chars().all(is_unicode_digit) {
        return None;
    }
    let ascii = to_ascii_digits(body);
    let v = BigInt::from_str(&ascii).ok()?;
    Some(if neg { -v } else { v })
}

/// Python's `float(str)` over a `_normalize`-shaped decimal string.
fn py_float(s: &str) -> Option<f64> {
    let t = s.trim_matches(py_is_space);
    let ascii = to_ascii_digits(t);
    ascii.parse::<f64>().ok()
}

/// `from .. import words2num; words2num(text, lang=lang)` — resolve the locale
/// and run its `to_cardinal`.
fn call_words2num(text: &str, lang: &str) -> Result<W2nValue, W2nError> {
    let resolved = resolve_lang(lang)?;
    converter_for(&resolved).to_cardinal(text)
}

/// Port of the public `words2num2.words2num(text, lang, to)` — the single-token
/// entry point, dispatch and all.
///
/// ```python
/// def words2num(text, lang="en", to="cardinal", **kwargs):
///     resolved = _resolve_lang(lang)
///     converter = CONVERTER_CLASSES[resolved]
///     if to not in CONVERTER_TYPES:
///         raise NotImplementedError("conversion type %r unsupported" % to)
///     return getattr(converter, "to_{}".format(to))(text, **kwargs)
/// ```
///
/// Returns the English grammar's [`crate::w2n_lang_en::W2nValue`] rather than
/// this module's [`W2nValue`], so the `en` decimal path keeps its `PyDec`
/// backing: a signed-zero decimal (`Decimal('-0.0')`) and the exact
/// 28-significant-digit `str()` both survive, neither of which `BigDecimal` can
/// carry. The 119 reverse-table locales only ever produce an `int` or `float`,
/// so mapping their result back through [`sentence_to_en_value`] is lossless.
pub fn words2num(
    text: &str,
    lang: &str,
    to: &str,
) -> Result<crate::w2n_lang_en::W2nValue, W2nError> {
    let resolved = resolve_lang(lang)?;
    let converter = converter_for(&resolved);

    // `CONVERTER_TYPES` in `words2num2/__init__.py`. An unknown `to` is a
    // `NotImplementedError`, distinct from the reverse table declining a word.
    const CONVERTER_TYPES: [&str; 6] =
        ["cardinal", "ordinal", "ordinal_num", "year", "currency", "digits"];
    if !CONVERTER_TYPES.contains(&to) {
        return Err(W2nError::NotImplemented(format!(
            "conversion type {} unsupported",
            py_repr_str(to)
        )));
    }

    // `getattr(converter, "to_{to}")(text)`. The English grammar path returns
    // its native value directly; every other path is `int`/`float` and is
    // promoted to the English value type.
    match &converter {
        Converter::En => match to {
            // `Words2Num_Base.to_currency` == `self.to_cardinal` (polymorphic).
            "cardinal" | "currency" => {
                crate::en_to_cardinal(text).map_err(|e| W2nError::Words2Num(e.msg))
            }
            "ordinal" => crate::en_to_ordinal(text).map_err(|e| W2nError::Words2Num(e.msg)),
            "year" => crate::en_to_year(text).map_err(|e| W2nError::Words2Num(e.msg)),
            "digits" => converter.to_digits(text).map(sentence_to_en_value),
            // `Words2Num_EN` inherits `Words2Num_Base.to_ordinal_num`.
            _ => base_ordinal_num(text).map(sentence_to_en_value),
        },
        Converter::Table(_) => {
            let v = match to {
                // `Words2Num_Base.to_year`/`to_currency` == `self.to_cardinal`.
                "cardinal" | "year" | "currency" => converter.to_cardinal(text),
                "ordinal" => converter.to_ordinal(text),
                "digits" => converter.to_digits(text),
                _ => converter.to_ordinal_num(text),
            }?;
            Ok(sentence_to_en_value(v))
        }
    }
}

/// Promote a reverse-table / `ordinal_num` result into the English grammar's
/// value type. Those paths never yield a `Decimal`, so the `Dec` arm is
/// unreachable in practice; it is mapped losslessly (rather than panicked on —
/// the crate builds `panic = "abort"`) via [`crate::w2n_lang_en::PyDec`].
fn sentence_to_en_value(v: W2nValue) -> crate::w2n_lang_en::W2nValue {
    use crate::w2n_lang_en::W2nValue as EnV;
    match v {
        W2nValue::Int(i) => EnV::Int(i),
        W2nValue::Float(f) => EnV::Float(f),
        W2nValue::Dec(d) => EnV::Dec(crate::w2n_lang_en::PyDec::from_bigdecimal(&d)),
    }
}

// ===========================================================================
// `words2num_sentence` / `convert_sentence` / `sentence_to_words`
// ===========================================================================

/// `SentenceConverter._starts_run` — a run must open with a real number word,
/// never with `"and"` / `"point"` / `"minus"`.
fn starts_run(converter: &Converter, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let dehyphened = token.replace('-', " ");
    py_split_whitespace(&dehyphened)
        .iter()
        .any(|sub| converter.is_number_word(sub))
}

/// `SentenceConverter._is_candidate` — cheap pre-filter for run growth.
fn is_candidate(converter: &Converter, token: &str, includable: &[&str]) -> bool {
    if token.is_empty() {
        return false;
    }
    if includable.contains(&token) {
        return true;
    }
    let dehyphened = token.replace('-', " ");
    py_split_whitespace(&dehyphened)
        .iter()
        .any(|sub| converter.is_number_word(sub))
}

/// `SentenceConverter.INCLUDABLE` — tokens allowed *inside* a run though they
/// are not numbers. Keyed by the **resolved** code, so only exactly `"en"`
/// gets them: `words2num_sentence(..., lang="en_IN")` resolves to `"en_IN"`
/// and therefore runs with an empty includable set.
const INCLUDABLE_EN: [&str; 7] = ["and", "point", "dot", "minus", "negative", "a", "an"];

/// Connector words allowed INSIDE a number run for non-English locales — the
/// analogue of English "and". Without them the walker breaks a run at the
/// connector ("sesenta **y** nueve mil ocho" stopped at "sesenta"), so
/// composed cardinals above the reverse table never reached the parser. Keyed
/// by resolved language prefix; empty for locales that don't join with a word.
fn includable_for(resolved: &str) -> &'static [&'static str] {
    let base = resolved.split(&['_', '-'][..]).next().unwrap_or(resolved);
    match base {
        "es" | "gl" => &["y", "e"],
        "pt" | "it" => &["e"],
        "fr" => &["et"],
        "ca" => &["i"],
        "de" => &["und"],
        "nl" | "af" => &["en"],
        "ro" => &["si", "și"],
        "pl" => &["i"],
        _ => &[],
    }
}

/// Port of `words2num2.words2num_sentence` → `SentenceConverter.convert`.
///
/// Walks the sentence and, at each position that opens with a real number
/// word, grows the longest run of tokens the per-language converter accepts.
///
/// `has_kwargs` is whether the Python caller passed extra keyword arguments
/// through (`kwargs or None` was truthy). The standard converters accept none,
/// so any such call fails every conversion — matching Python's swallowed
/// `TypeError`.
pub fn words2num_sentence(
    sentence: &str,
    lang: &str,
    to: &str,
    has_kwargs: bool,
) -> Result<String, W2nError> {
    let resolved = resolve_lang(lang)?;
    let converter = converter_for(&resolved);
    let includable: &[&str] = if resolved == "en" {
        &INCLUDABLE_EN
    } else {
        includable_for(&resolved)
    };

    let parts = tokenize(sentence);
    let n = parts.len();
    let mut out = String::new();
    let mut i = 0usize;

    while i < n {
        let piece = &parts[i];
        if py_str_isspace(piece) {
            out.push_str(piece);
            i += 1;
            continue;
        }
        // A run must START with a real number word.
        let head = rstrip_punct(piece).to_lowercase();
        if !starts_run(&converter, &head) {
            out.push_str(piece);
            i += 1;
            continue;
        }

        // Grow a number run starting at i.
        let mut best_value: Option<W2nValue> = None;
        let mut best_end = i;
        let mut j = i;
        while j < n {
            let tok = &parts[j];
            if py_str_isspace(tok) {
                j += 1;
                continue;
            }
            let clean = rstrip_punct(tok).to_lowercase();
            if !is_candidate(&converter, &clean, includable) {
                break;
            }
            let run = parts[i..=j].concat();
            let stripped = rstrip_punct(py_strip(&run)).to_string();
            // The standard converters never return `None`, so a successful
            // parse always both records the value and advances best_end. A
            // raised error is Python's `except Exception` — swallow and keep
            // growing.
            if let Ok(v) = converter.convert(to, &stripped, has_kwargs) {
                best_value = Some(v);
                best_end = j;
            }
            // A token ending in terminal punctuation closes the run.
            if ends_with_terminal_punct(tok) {
                break;
            }
            j += 1;
        }

        if let Some(v) = best_value {
            // Preserve trailing punctuation that was stripped during parse.
            let run = parts[i..=best_end].concat();
            let trailing = trailing_punct(&run);
            out.push_str(&v.py_str());
            out.push_str(trailing);
            i = best_end + 1;
        } else {
            out.push_str(piece);
            i += 1;
        }
    }
    Ok(out)
}

/// `convert_sentence = words2num_sentence` (alias, parity with num2words2).
pub fn convert_sentence(
    sentence: &str,
    lang: &str,
    to: &str,
    has_kwargs: bool,
) -> Result<String, W2nError> {
    words2num_sentence(sentence, lang, to, has_kwargs)
}

/// `sentence_to_words = words2num_sentence` (alias, parity with num2words2).
pub fn sentence_to_words(
    sentence: &str,
    lang: &str,
    to: &str,
    has_kwargs: bool,
) -> Result<String, W2nError> {
    words2num_sentence(sentence, lang, to, has_kwargs)
}

// ===========================================================================
// Registries — `UNITS`, `CURRENCIES`, `SCALE_SUFFIXES`
// ===========================================================================

/// `converters.auto.UnitInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitInfo {
    pub short: &'static str,
    pub long: &'static str,
    pub kind: &'static str,
    pub confidence: f64,
}

const fn ui(short: &'static str, long: &'static str, kind: &'static str, confidence: f64) -> UnitInfo {
    UnitInfo {
        short,
        long,
        kind,
        confidence,
    }
}

/// `converters.auto.UNITS`.
///
/// A slice, not a map: insertion order is observable through
/// `next((u.long for u in UNITS.values() if u.short == short), tail)` in
/// `_try_word_unit`, which takes the *first* entry with a matching short form
/// (`"l"` before `"L"` → `"liter"`; `"µm"` before `"um"` → `"micrometer"`;
/// `"°C"` before `"C"` → `"degree celsius"`).
///
/// The `µ` here is U+00B5 MICRO SIGN, matching the source (not U+03BC).
pub static UNITS: &[(&str, UnitInfo)] = &[
    // Length
    ("mm", ui("mm", "millimeter", "length", 1.0)),
    ("cm", ui("cm", "centimeter", "length", 1.0)),
    ("dm", ui("dm", "decimeter", "length", 1.0)),
    ("m", ui("m", "meter", "length", 0.6)),
    ("km", ui("km", "kilometer", "length", 1.0)),
    ("in", ui("in", "inch", "length", 0.5)),
    ("ft", ui("ft", "foot", "length", 1.0)),
    ("yd", ui("yd", "yard", "length", 1.0)),
    ("mi", ui("mi", "mile", "length", 1.0)),
    ("nm", ui("nm", "nanometer", "length", 1.0)),
    ("\u{b5}m", ui("\u{b5}m", "micrometer", "length", 1.0)),
    ("um", ui("\u{b5}m", "micrometer", "length", 1.0)),
    // Mass
    ("mg", ui("mg", "milligram", "mass", 1.0)),
    ("g", ui("g", "gram", "mass", 0.6)),
    ("kg", ui("kg", "kilogram", "mass", 1.0)),
    ("t", ui("t", "tonne", "mass", 0.5)),
    ("lb", ui("lb", "pound", "mass", 1.0)),
    ("lbs", ui("lb", "pound", "mass", 1.0)),
    ("oz", ui("oz", "ounce", "mass", 1.0)),
    // Temperature
    ("\u{b0}", ui("\u{b0}", "degree", "temperature", 0.7)),
    ("\u{b0}C", ui("\u{b0}C", "degree celsius", "temperature", 1.0)),
    ("\u{b0}F", ui("\u{b0}F", "degree fahrenheit", "temperature", 1.0)),
    ("K", ui("K", "kelvin", "temperature", 0.6)),
    ("C", ui("\u{b0}C", "degree celsius", "temperature", 0.5)),
    ("F", ui("\u{b0}F", "degree fahrenheit", "temperature", 0.5)),
    // Time
    ("ms", ui("ms", "millisecond", "time", 1.0)),
    ("s", ui("s", "second", "time", 0.6)),
    ("sec", ui("s", "second", "time", 1.0)),
    ("min", ui("min", "minute", "time", 1.0)),
    ("h", ui("h", "hour", "time", 0.7)),
    ("hr", ui("h", "hour", "time", 1.0)),
    ("hrs", ui("h", "hour", "time", 1.0)),
    ("d", ui("d", "day", "time", 0.5)),
    // Volume
    ("ml", ui("ml", "milliliter", "volume", 1.0)),
    ("cl", ui("cl", "centiliter", "volume", 1.0)),
    ("dl", ui("dl", "deciliter", "volume", 1.0)),
    ("l", ui("L", "liter", "volume", 0.7)),
    ("L", ui("L", "liter", "volume", 1.0)),
    ("gal", ui("gal", "gallon", "volume", 1.0)),
    // Percent
    ("%", ui("%", "percent", "percent", 1.0)),
];

/// `UNITS.get(key)`.
pub fn units_get(key: &str) -> Option<&'static UnitInfo> {
    UNITS.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

/// `converters.auto.CurrencyInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct CurrencyInfo {
    pub code: &'static str,
    pub symbol: &'static str,
    pub long: &'static str,
}

const fn ci(code: &'static str, symbol: &'static str, long: &'static str) -> CurrencyInfo {
    CurrencyInfo { code, symbol, long }
}

/// `converters.auto.CURRENCIES`.
///
/// Note `"EUR"` maps to long `"euro"` while `"USD"` maps to `"US dollar"` but
/// the `"$"` symbol maps to `"dollar"` — so `auto_parse_sentence("$5", expand=True)`
/// says "dollars" and `auto_parse_sentence("5 USD", expand=True)` says
/// "US dollars". Faithful to the source.
pub static CURRENCIES: &[(&str, CurrencyInfo)] = &[
    ("$", ci("USD", "$", "dollar")),
    ("\u{20ac}", ci("EUR", "\u{20ac}", "euro")),
    ("\u{a3}", ci("GBP", "\u{a3}", "pound")),
    ("\u{a5}", ci("JPY", "\u{a5}", "yen")),
    ("\u{20b9}", ci("INR", "\u{20b9}", "rupee")),
    ("\u{20bd}", ci("RUB", "\u{20bd}", "ruble")),
    ("\u{20a9}", ci("KRW", "\u{20a9}", "won")),
    ("\u{20ba}", ci("TRY", "\u{20ba}", "lira")),
    ("USD", ci("USD", "$", "US dollar")),
    ("EUR", ci("EUR", "\u{20ac}", "euro")),
    ("GBP", ci("GBP", "\u{a3}", "pound sterling")),
    ("JPY", ci("JPY", "\u{a5}", "yen")),
    ("CHF", ci("CHF", "CHF", "Swiss franc")),
    ("CAD", ci("CAD", "$", "Canadian dollar")),
    ("AUD", ci("AUD", "$", "Australian dollar")),
    ("CNY", ci("CNY", "\u{a5}", "yuan")),
    ("INR", ci("INR", "\u{20b9}", "rupee")),
    ("BRL", ci("BRL", "R$", "real")),
    ("MXN", ci("MXN", "$", "Mexican peso")),
    ("RUB", ci("RUB", "\u{20bd}", "ruble")),
    ("KRW", ci("KRW", "\u{20a9}", "won")),
];

/// `CURRENCIES.get(key)`.
pub fn currencies_get(key: &str) -> Option<&'static CurrencyInfo> {
    CURRENCIES.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

/// The eight symbols in the `[$€£¥₹₽₩₺]` character class.
const CURRENCY_SYMBOLS: [char; 8] = [
    '$',
    '\u{20ac}',
    '\u{a3}',
    '\u{a5}',
    '\u{20b9}',
    '\u{20bd}',
    '\u{20a9}',
    '\u{20ba}',
];

fn is_currency_symbol(c: char) -> bool {
    CURRENCY_SYMBOLS.contains(&c)
}

/// `converters.auto.SCALE_SUFFIXES`. Returns the multiplier, or `None` for a
/// `KeyError` — the regex admits combinations this table does not hold
/// (`"kn"`, `"BN"`, `"Tn"`, `"bN"` …), and Python genuinely raises there.
fn scale_suffix(s: &str) -> Option<BigInt> {
    let pow = |n: u32| BigInt::from(10u8).pow(n);
    match s {
        "k" | "K" => Some(pow(3)),
        "m" | "M" => Some(pow(6)),
        "b" | "B" | "bn" => Some(pow(9)),
        "t" | "T" | "tn" => Some(pow(12)),
        _ => None,
    }
}

// ===========================================================================
// `Quantity`
// ===========================================================================

/// `converters.auto.Quantity`.
#[derive(Debug, Clone, PartialEq)]
pub struct Quantity {
    pub value: W2nValue,
    pub unit: Option<String>,
    pub unit_long: Option<String>,
    pub kind: Option<String>,
    pub confidence: f64,
    pub raw: String,
}

impl Quantity {
    fn bare(value: W2nValue, raw: &str) -> Quantity {
        Quantity {
            value,
            unit: None,
            unit_long: None,
            kind: None,
            confidence: 1.0,
            raw: raw.to_string(),
        }
    }

    /// `Quantity.__repr__`.
    ///
    /// ```python
    /// if self.unit:
    ///     return "Quantity(value={!r}, unit={!r}, kind={!r}, confidence={})".format(...)
    /// return "Quantity(value={!r})".format(self.value)
    /// ```
    ///
    /// `if self.unit` is a *truthiness* test, so an empty-string unit takes
    /// the short branch. `confidence` uses `{}` (i.e. `str`), not `{!r}`.
    pub fn py_repr(&self) -> String {
        match &self.unit {
            Some(u) if !u.is_empty() => format!(
                "Quantity(value={}, unit={}, kind={}, confidence={})",
                self.value.py_repr(),
                py_repr_str(u),
                match &self.kind {
                    Some(k) => py_repr_str(k),
                    None => "None".to_string(),
                },
                py_float_repr(self.confidence),
            ),
            _ => format!("Quantity(value={})", self.value.py_repr()),
        }
    }
}

// ===========================================================================
// `formats.py` — DUPLICATED HERE ON PURPOSE
// ===========================================================================
//
// `auto_parse` cannot run without `parse_number_string`. These private helpers
// are a self-contained copy that stays byte-consistent with this module's own
// Unicode digit set ([`ND_BLOCKS`]); the canonical crate-level port lives in
// [`crate::w2n_formats`]. Both are verified against the interpreter.

/// `formats.NUMBER_FORMAT_DEFAULTS` — (lang, thousands, decimal).
const NUMBER_FORMAT_DEFAULTS: &[(&str, &str, &str)] = &[
    ("_default", ",", "."),
    ("en", ",", "."),
    ("en_GB", ",", "."),
    ("en_IN", ",", "."),
    ("en_NG", ",", "."),
    ("zh", ",", "."),
    ("zh_CN", ",", "."),
    ("zh_HK", ",", "."),
    ("zh_TW", ",", "."),
    ("ja", ",", "."),
    ("ko", ",", "."),
    ("th", ",", "."),
    ("vi", ".", ","),
    ("fr", " ", ","),
    ("fr_BE", " ", ","),
    ("fr_DZ", " ", ","),
    ("fr_CH", "'", "."),
    ("de", ".", ","),
    ("es", ".", ","),
    ("es_CO", ".", ","),
    ("es_CR", ".", ","),
    ("es_GT", ".", ","),
    ("es_NI", ".", ","),
    ("es_VE", ".", ","),
    ("it", ".", ","),
    ("pt", ".", ","),
    ("pt_BR", ".", ","),
    ("nl", ".", ","),
    ("ro", ".", ","),
    ("hr", ".", ","),
    ("sl", ".", ","),
    ("sr", ".", ","),
    ("tr", ".", ","),
    ("el", ".", ","),
    ("ru", " ", ","),
    ("uk", " ", ","),
    ("be", " ", ","),
    ("bg", " ", ","),
    ("pl", " ", ","),
    ("cs", " ", ","),
    ("sk", " ", ","),
    ("hu", " ", ","),
    ("sv", " ", ","),
    ("no", " ", ","),
    ("nn", " ", ","),
    ("da", ".", ","),
    ("fi", " ", ","),
    ("et", " ", ","),
    ("lt", " ", ","),
    ("lv", " ", ","),
    ("is", ".", ","),
    ("fo", ".", ","),
    ("ar", ",", "."),
    ("fa", ",", "."),
    ("he", ",", "."),
];

/// `formats.get_format`.
fn get_format(lang: &str) -> (&'static str, &'static str) {
    let find = |k: &str| {
        NUMBER_FORMAT_DEFAULTS
            .iter()
            .find(|(n, _, _)| *n == k)
            .map(|(_, t, d)| (*t, *d))
    };
    if let Some(f) = find(lang) {
        return f;
    }
    // `lang.split("_")[0] if lang else ""` — `lang` is never None here.
    let base = lang.split('_').next().unwrap_or("");
    if !lang.is_empty() {
        if let Some(f) = find(base) {
            return f;
        }
    }
    find("_default").expect("_default is always present")
}

/// `formats._to_number` — the only place an int/float distinction is made.
fn to_number(s: &str) -> Result<W2nValue, W2nError> {
    // re.fullmatch(r"\d+(?:\.\d+)?", s)
    let ok = {
        let mut it = s.chars().peekable();
        let mut n_int = 0usize;
        while it.peek().copied().is_some_and(is_unicode_digit) {
            it.next();
            n_int += 1;
        }
        let mut good = n_int > 0;
        if good && it.peek() == Some(&'.') {
            it.next();
            let mut n_frac = 0usize;
            while it.peek().copied().is_some_and(is_unicode_digit) {
                it.next();
                n_frac += 1;
            }
            good = n_frac > 0;
        }
        good && it.next().is_none()
    };
    if !ok {
        return Err(W2nError::Words2Num(format!(
            "not a parseable number: {}",
            py_repr_str(s)
        )));
    }
    // int()/float() fold Nd digits to their decimal value.
    let ascii = to_ascii_digits(s);
    if ascii.contains('.') {
        Ok(W2nValue::Float(ascii.parse::<f64>().map_err(|e| {
            W2nError::Words2Num(e.to_string())
        })?))
    } else {
        Ok(W2nValue::Int(BigInt::from_str(&ascii).map_err(|e| {
            W2nError::Words2Num(e.to_string())
        })?))
    }
}

/// `re.sub(r"[\s\xa0  ]", "", s)` — `\s` already covers all three
/// named spaces, so this is `\s` plus belt-and-braces.
fn strip_space_like(s: &str) -> String {
    s.chars()
        .filter(|c| !(py_is_space(*c) || matches!(c, '\u{a0}' | '\u{202f}' | '\u{2009}')))
        .collect()
}

/// `formats._parse_with_explicit`.
fn parse_with_explicit(
    s: &str,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
) -> Result<W2nValue, W2nError> {
    let mut s = s.to_string();
    // `if thousands_sep:` — truthiness, so an empty separator is skipped.
    if let Some(t) = thousands_sep.filter(|t| !t.is_empty()) {
        if matches!(t, " " | "\u{a0}" | "\u{202f}" | "\u{2009}") {
            s = strip_space_like(&s);
        } else {
            s = s.replace(t, "");
        }
    }
    if let Some(d) = decimal_sep.filter(|d| !d.is_empty()) {
        if d != "." {
            if s.contains('.') {
                // A non-dot decimal was configured but dots are present —
                // Python drops them as stray separators. This is why
                // parse_number_string("0.5", lang="fr") == 5.
                s = s.replace('.', "");
            }
            s = s.replace(d, ".");
        }
    }
    to_number(&s)
}

/// `formats._resolve_single_sep`.
fn resolve_single_sep(s: &str, sep: char) -> String {
    let parts: Vec<&str> = s.split(sep).collect();
    if parts.len() > 2 {
        return s.replace(sep, "");
    }
    let after = parts[1]; // len(parts) >= 2 — the caller only calls when sep is present
    let n_after = after.chars().count();
    if n_after == 3 && !parts[0].is_empty() && parts[0].chars().count() <= 3 {
        if sep == '.' {
            return s.to_string();
        }
        return s.replace(',', ".");
    }
    if n_after != 3 {
        return if sep == '.' {
            s.to_string()
        } else {
            s.replace(',', ".")
        };
    }
    s.replace(sep, "")
}

/// `formats._auto_detect_parse`.
fn auto_detect_parse(s: &str) -> Result<W2nValue, W2nError> {
    // re.sub(r"[\s\xa0  '_]", "", s)
    let s2: String = s
        .chars()
        .filter(|c| {
            !(py_is_space(*c) || matches!(c, '\u{a0}' | '\u{202f}' | '\u{2009}' | '\'' | '_'))
        })
        .collect();
    let has_comma = s2.contains(',');
    let has_dot = s2.contains('.');

    if has_comma && has_dot {
        // Both present — the rightmost is the decimal.
        let last_comma = s2.rfind(',');
        let last_dot = s2.rfind('.');
        let s3 = if last_comma > last_dot {
            s2.replace('.', "").replace(',', ".")
        } else {
            s2.replace(',', "")
        };
        return to_number(&s3);
    }
    if has_comma {
        return to_number(&resolve_single_sep(&s2, ','));
    }
    if has_dot {
        return to_number(&resolve_single_sep(&s2, '.'));
    }
    to_number(&s2)
}

/// `sign * value`, where `sign` is Python's `int` 1 or -1.
/// `int * int -> int`; `int * float -> float`.
fn apply_sign(sign: i32, v: W2nValue) -> W2nValue {
    if sign == 1 {
        return v;
    }
    match v {
        W2nValue::Int(i) => W2nValue::Int(-i),
        W2nValue::Float(f) => W2nValue::Float(-f),
        W2nValue::Dec(d) => W2nValue::Dec(-d),
    }
}

/// `formats.parse_number_string`.
fn parse_number_string(
    s: &str,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
    lang: Option<&str>,
) -> Result<W2nValue, W2nError> {
    let s = py_strip(s);
    if s.is_empty() {
        return Err(W2nError::Words2Num("empty numeric string".to_string()));
    }
    let mut sign = 1i32;
    let mut rest = s;
    let first = s.chars().next().unwrap();
    if first == '+' || first == '-' {
        if first == '-' {
            sign = -1;
        }
        rest = py_strip_start(&s[first.len_utf8()..]);
    }
    if rest.is_empty() {
        return Err(W2nError::Words2Num(
            "empty numeric string after sign".to_string(),
        ));
    }
    if thousands_sep.is_some() || decimal_sep.is_some() {
        return Ok(apply_sign(
            sign,
            parse_with_explicit(rest, thousands_sep, decimal_sep)?,
        ));
    }
    if let Some(l) = lang {
        let (t, d) = get_format(l);
        match parse_with_explicit(rest, Some(t), Some(d)) {
            Ok(v) => return Ok(apply_sign(sign, v)),
            Err(W2nError::Words2Num(_)) => {} // fall through to auto-detect
            Err(e) => return Err(e),
        }
    }
    Ok(apply_sign(sign, auto_detect_parse(rest)?))
}

/// Python's `str.lstrip()`.
fn py_strip_start(s: &str) -> &str {
    s.trim_start_matches(py_is_space)
}

// ===========================================================================
// `auto_parse` internals
// ===========================================================================

/// The `[\d.,'\xa0   _]` class shared by every `auto.py` regex.
fn is_num_class(c: char) -> bool {
    is_unicode_digit(c)
        || matches!(
            c,
            '.' | ',' | '\'' | '\u{a0}' | '\u{202f}' | '\u{2009}' | ' ' | '_'
        )
}

/// `[.,'_\xa0   ]` — the same eight non-digit characters, used as
/// the internal separator in `auto_parse_sentence`'s `NUM`.
fn is_num_sep(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | '\'' | '_' | '\u{a0}' | '\u{202f}' | '\u{2009}' | ' '
    )
}

/// A tiny cursor over `&[char]`, since these regexes must be matched by
/// character (the classes contain non-ASCII) and never by byte.
struct Cursor<'a> {
    c: &'a [char],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(c: &'a [char]) -> Self {
        Cursor { c, i: 0 }
    }
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    fn at(&self, k: usize) -> Option<char> {
        self.c.get(self.i + k).copied()
    }
    fn eat_if(&mut self, f: impl Fn(char) -> bool) -> bool {
        if self.peek().is_some_and(&f) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    /// `\s*`
    fn skip_ws(&mut self) {
        while self.eat_if(py_is_space) {}
    }
    fn take_while(&mut self, f: impl Fn(char) -> bool) -> String {
        let start = self.i;
        while self.peek().is_some_and(&f) {
            self.i += 1;
        }
        self.c[start..self.i].iter().collect()
    }
    fn at_end(&self) -> bool {
        self.i >= self.c.len()
    }
}

/// `([-+]?[\d.,'\xa0   _]+)`, greedy.
///
/// No backtracking is needed anywhere this is used: the only characters the
/// greedy run could give back that a following `\s*` would accept are the four
/// space-likes, and `\s*` would then consume them and land on the very same
/// index. Every continuation is therefore identical.
fn eat_signed_num_class(cur: &mut Cursor) -> Option<String> {
    let start = cur.i;
    cur.eat_if(|c| c == '-' || c == '+');
    let body = cur.take_while(is_num_class);
    if body.is_empty() {
        cur.i = start;
        return None;
    }
    Some(cur.c[start..cur.i].iter().collect())
}

/// `([A-Z]{3})`
fn eat_iso_code(cur: &mut Cursor) -> Option<String> {
    let mut out = String::new();
    for k in 0..3 {
        match cur.at(k) {
            Some(c) if c.is_ascii_uppercase() => out.push(c),
            _ => return None,
        }
    }
    cur.i += 3;
    Some(out)
}

/// `converters.auto._try_currency_prefix`
///
/// ```python
/// m = re.match(r"^\s*([$€£¥₹₽₩₺])\s*([-+]?[\d.,'\xa0   _]+)"
///              r"\s*([kKmMbBtT][nN]?)?\s*$", text)
/// ...
/// m = re.match(r"^\s*([A-Z]{3})\s*([-+]?[\d.,'\xa0   _]+)\s*$", text)
/// if m and m.group(1) in CURRENCIES: ...
/// ```
fn try_currency_prefix(text: &[char]) -> Option<(String, String, Option<String>)> {
    // Symbol prefix.
    {
        let mut cur = Cursor::new(text);
        cur.skip_ws();
        if let Some(sym) = cur.peek().filter(|c| is_currency_symbol(*c)) {
            cur.i += 1;
            cur.skip_ws();
            if let Some(num) = eat_signed_num_class(&mut cur) {
                cur.skip_ws();
                let mut scale = None;
                if cur.peek().is_some_and(|c| "kKmMbBtT".contains(c)) {
                    let mut s = String::new();
                    s.push(cur.peek().unwrap());
                    cur.i += 1;
                    if cur.peek().is_some_and(|c| c == 'n' || c == 'N') {
                        s.push(cur.peek().unwrap());
                        cur.i += 1;
                    }
                    scale = Some(s);
                }
                cur.skip_ws();
                if cur.at_end() {
                    return Some((sym.to_string(), py_strip(&num).to_string(), scale));
                }
            }
        }
    }
    // 3-letter ISO code prefix (USD, EUR, ...).
    {
        let mut cur = Cursor::new(text);
        cur.skip_ws();
        if let Some(code) = eat_iso_code(&mut cur) {
            cur.skip_ws();
            if let Some(num) = eat_signed_num_class(&mut cur) {
                cur.skip_ws();
                if cur.at_end() && currencies_get(&code).is_some() {
                    return Some((code, py_strip(&num).to_string(), None));
                }
            }
        }
    }
    None
}

/// `converters.auto._try_currency_suffix`
fn try_currency_suffix(text: &[char]) -> Option<(String, String)> {
    // Symbol suffix.
    {
        let mut cur = Cursor::new(text);
        cur.skip_ws();
        if let Some(num) = eat_signed_num_class(&mut cur) {
            cur.skip_ws();
            if let Some(sym) = cur.peek().filter(|c| is_currency_symbol(*c)) {
                cur.i += 1;
                cur.skip_ws();
                if cur.at_end() {
                    return Some((sym.to_string(), py_strip(&num).to_string()));
                }
            }
        }
    }
    // 3-letter ISO code suffix.
    {
        let mut cur = Cursor::new(text);
        cur.skip_ws();
        if let Some(num) = eat_signed_num_class(&mut cur) {
            cur.skip_ws();
            if let Some(code) = eat_iso_code(&mut cur) {
                cur.skip_ws();
                if cur.at_end() && currencies_get(&code).is_some() {
                    return Some((code, py_strip(&num).to_string()));
                }
            }
        }
    }
    None
}

/// `converters.auto._quantity_currency`
fn quantity_currency(value: W2nValue, sym: &str, raw: &str) -> Quantity {
    let info = currencies_get(sym).expect("every symbol the regex matches is a CURRENCIES key");
    Quantity {
        value,
        unit: Some(info.code.to_string()),
        unit_long: Some(info.long.to_string()),
        kind: Some("currency".to_string()),
        confidence: 1.0,
        raw: raw.to_string(),
    }
}

/// `converters.auto._apply_scale`
///
/// ```python
/// multiplier = SCALE_SUFFIXES[scale_str]   # KeyError escapes auto_parse
/// if isinstance(value, int): return value * multiplier
/// return float(value) * multiplier
/// ```
fn apply_scale(value: W2nValue, scale_str: &str) -> Result<W2nValue, W2nError> {
    let multiplier =
        scale_suffix(scale_str).ok_or_else(|| W2nError::Key(scale_str.to_string()))?;
    match value {
        W2nValue::Int(i) => Ok(W2nValue::Int(i * multiplier)),
        // float(value) * multiplier — Python widens the int multiplier.
        W2nValue::Float(f) => Ok(W2nValue::Float(f * bigint_to_f64(&multiplier))),
        W2nValue::Dec(d) => Ok(W2nValue::Float(
            bigdecimal_to_f64(&d) * bigint_to_f64(&multiplier),
        )),
    }
}

fn bigint_to_f64(i: &BigInt) -> f64 {
    i.to_string().parse::<f64>().unwrap_or(f64::INFINITY)
}

fn bigdecimal_to_f64(d: &BigDecimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(f64::INFINITY)
}

/// `converters.auto._resolve_unit`
///
/// Returns `(info, override_long, confidence)`; `None` means the token is not
/// a unit at all.
fn resolve_unit(
    unit_str: &str,
    prefer: &HashMap<String, String>,
) -> Option<(&'static UnitInfo, Option<&'static str>, f64)> {
    // Direct hit, then a lowercased retry (so "°C"/"K" keep their casing).
    let info = units_get(unit_str).or_else(|| units_get(&unit_str.to_lowercase()))?;

    // prefer-override is keyed on the ORIGINAL token, not the lowercased one.
    if let Some(target) = prefer.get(unit_str).filter(|t| !t.is_empty()) {
        for (_, u) in UNITS.iter() {
            if u.long == target || u.short == target {
                // Keep the original short token, flip the long form, and clamp
                // confidence to reflect the caller stepping in.
                return Some((info, Some(u.long), info.confidence.max(0.9)));
            }
        }
    }
    Some((info, None, info.confidence))
}

/// `converters.auto._try_digit_unit`
///
/// ```python
/// m = re.match(r"^\s*([-+]?[\d.,'\xa0   _]+)\s*(°[CF]?|°[CF]?|µm|"
///              r"[a-zA-Z]+|%)\s*$", text)
/// ```
/// The `°[CF]?` branch is duplicated in the source; the second is dead.
fn try_digit_unit(
    text: &[char],
    prefer: &HashMap<String, String>,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
    lang: &str,
) -> Result<Option<Quantity>, W2nError> {
    let mut cur = Cursor::new(text);
    cur.skip_ws();
    let Some(num) = eat_signed_num_class(&mut cur) else {
        return Ok(None);
    };
    cur.skip_ws();

    // Ordered alternation: °[CF]? | °[CF]? | µm | [a-zA-Z]+ | %
    let unit: String = if cur.peek() == Some('\u{b0}') {
        let mut s = String::from('\u{b0}');
        cur.i += 1;
        if let Some(c) = cur.peek().filter(|c| *c == 'C' || *c == 'F') {
            s.push(c);
            cur.i += 1;
        }
        s
    } else if cur.peek() == Some('\u{b5}') && cur.at(1) == Some('m') {
        cur.i += 2;
        String::from("\u{b5}m")
    } else if cur.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
        cur.take_while(|c| c.is_ascii_alphabetic())
    } else if cur.peek() == Some('%') {
        cur.i += 1;
        String::from("%")
    } else {
        return Ok(None);
    };
    cur.skip_ws();
    if !cur.at_end() {
        return Ok(None);
    }

    let num_str = py_strip(&num).to_string();
    let unit_str = py_strip(&unit).to_string();

    let Some((info, alt_long, conf)) = resolve_unit(&unit_str, prefer) else {
        return Ok(None);
    };
    let value = match parse_number_string(&num_str, thousands_sep, decimal_sep, Some(lang)) {
        Ok(v) => v,
        Err(W2nError::Words2Num(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(Some(Quantity {
        value,
        unit: Some(info.short.to_string()),
        unit_long: Some(alt_long.unwrap_or(info.long).to_string()),
        kind: Some(info.kind.to_string()),
        confidence: conf,
        raw: String::new(),
    }))
}

/// `converters.auto._try_word_unit`'s `word_units` table → `(short, kind)`.
///
/// `"pound sterling"` is present in the source but unreachable: the lookup key
/// comes from `rsplit(None, 1)` and so can never contain a space. Kept for
/// fidelity of the table, not of the behaviour.
fn word_unit(tail: &str) -> Option<(&'static str, &'static str)> {
    Some(match tail {
        "millimeter" | "millimetre" => ("mm", "length"),
        "centimeter" | "centimetre" => ("cm", "length"),
        "meter" | "metre" => ("m", "length"),
        "kilometer" | "kilometre" => ("km", "length"),
        "inch" => ("in", "length"),
        "foot" | "feet" => ("ft", "length"),
        "yard" => ("yd", "length"),
        "mile" => ("mi", "length"),
        "milligram" => ("mg", "mass"),
        "gram" => ("g", "mass"),
        "kilogram" => ("kg", "mass"),
        "tonne" | "ton" => ("t", "mass"),
        "pound" => ("lb", "mass"),
        "ounce" => ("oz", "mass"),
        "second" => ("s", "time"),
        "minute" => ("min", "time"),
        "hour" => ("h", "time"),
        "day" => ("d", "time"),
        "millisecond" => ("ms", "time"),
        "milliliter" | "millilitre" => ("ml", "volume"),
        "liter" | "litre" => ("L", "volume"),
        "gallon" => ("gal", "volume"),
        "percent" => ("%", "percent"),
        "degree" => ("\u{b0}", "temperature"),
        "celsius" => ("\u{b0}C", "temperature"),
        "fahrenheit" => ("\u{b0}F", "temperature"),
        "kelvin" => ("K", "temperature"),
        // currency words
        "dollar" => ("USD", "currency"),
        "euro" => ("EUR", "currency"),
        "pound sterling" => ("GBP", "currency"), // unreachable, see above
        "yen" => ("JPY", "currency"),
        "rupee" => ("INR", "currency"),
        "yuan" => ("CNY", "currency"),
        "franc" => ("CHF", "currency"),
        "ruble" => ("RUB", "currency"),
        "won" => ("KRW", "currency"),
        _ => return None,
    })
}

/// `converters.auto._try_word_unit` — a word-form number plus a unit word.
fn try_word_unit(
    text: &str,
    lang: &str,
    prefer: &HashMap<String, String>,
) -> Result<Option<Quantity>, W2nError> {
    let parts = py_rsplit_once_ws(text);
    if parts.len() != 2 {
        return Ok(None);
    }
    let head = parts[0];
    let raw_tail = parts[1];
    // `parts[1].lower().rstrip("s")` — rstrip drops EVERY trailing "s".
    let tail = py_rstrip_char(&raw_tail.to_lowercase(), 's').to_string();

    let (short, kind) = match word_unit(&tail) {
        Some(v) => v,
        None => {
            // Fall back to short-form tokens (kg, cm, °C …) so mixed forms
            // like "forty-two kg" work. NB: `_resolve_unit` is handed the
            // ORIGINAL tail, not the lowercased/rstripped one — which is why
            // "five lbs" works (UNITS has "lbs") but "forty-two kgs" does not.
            match resolve_unit(raw_tail, prefer) {
                Some((info, _alt, _conf)) => (info.short, info.kind),
                None => return Ok(None),
            }
        }
    };

    // `words2num(head, lang=lang)` — a raised Exception (unparseable /
    // unsupported locale) is swallowed to "not a unit expression".
    let value = match call_words2num(head, lang) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    if kind == "currency" {
        let info = currencies_get(short).expect("every currency short is a CURRENCIES key");
        return Ok(Some(Quantity {
            value,
            unit: Some(info.code.to_string()),
            unit_long: Some(info.long.to_string()),
            kind: Some("currency".to_string()),
            confidence: 1.0,
            raw: String::new(),
        }));
    }
    // `next((u.long for u in UNITS.values() if u.short == short), tail)` —
    // first match in insertion order, falling back to the bare tail.
    let long_name = UNITS
        .iter()
        .find(|(_, u)| u.short == short)
        .map(|(_, u)| u.long.to_string())
        .unwrap_or_else(|| tail.clone());
    Ok(Some(Quantity {
        value,
        unit: Some(short.to_string()),
        unit_long: Some(long_name),
        kind: Some(kind.to_string()),
        confidence: 1.0,
        raw: String::new(),
    }))
}

// ===========================================================================
// `auto_parse`
// ===========================================================================

/// Port of `converters.auto.auto_parse`.
///
/// Tries, in order: currency prefix, currency suffix, digit+unit, word+unit,
/// bare digit number, bare word number.
///
/// The `isinstance(text, str)` guard has no analogue here — `text` is `&str`.
/// A binding that accepts `PyAny` must reproduce
/// `Words2NumError("expected str, got %r" % type(text).__name__)` itself.
pub fn auto_parse(
    text: &str,
    lang: &str,
    prefer: &HashMap<String, String>,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
) -> Result<Quantity, W2nError> {
    let raw = text;
    let text = py_strip(text);
    if text.is_empty() {
        return Err(W2nError::Words2Num("empty input".to_string()));
    }
    let chars: Vec<char> = text.chars().collect();

    // 1. Currency-prefix form: $12.50 / €12,50 / $5m
    if let Some((sym, num_str, scale)) = try_currency_prefix(&chars) {
        // NB: only Words2NumError is swallowed here. A KeyError out of
        // _apply_scale ("$5kn") escapes auto_parse entirely.
        match (|| -> Result<Quantity, W2nError> {
            let mut value = parse_number_string(&num_str, thousands_sep, decimal_sep, Some(lang))?;
            if let Some(s) = scale.as_deref().filter(|s| !s.is_empty()) {
                value = apply_scale(value, s)?;
            }
            Ok(quantity_currency(value, &sym, raw))
        })() {
            Ok(q) => return Ok(q),
            Err(W2nError::Words2Num(_)) => {}
            Err(e) => return Err(e),
        }
    }

    // 2. Currency-suffix form: 12.50 € / 45 USD
    if let Some((sym, num_str)) = try_currency_suffix(&chars) {
        match parse_number_string(&num_str, thousands_sep, decimal_sep, Some(lang)) {
            Ok(value) => return Ok(quantity_currency(value, &sym, raw)),
            Err(W2nError::Words2Num(_)) => {}
            Err(e) => return Err(e),
        }
    }

    // 3. Number + unit suffix (digit form): "5cm", "20°C", "42%", "3.5 km"
    if let Some(mut res) = try_digit_unit(&chars, prefer, thousands_sep, decimal_sep, lang)? {
        res.raw = raw.to_string();
        return Ok(res);
    }

    // 4. Word-form number + unit: "forty-two kg", "twenty-three percent"
    if let Some(mut res) = try_word_unit(text, lang, prefer)? {
        res.raw = raw.to_string();
        return Ok(res);
    }

    // 5. Pure number — digit form, then word form.
    match parse_number_string(text, thousands_sep, decimal_sep, Some(lang)) {
        Ok(value) => return Ok(Quantity::bare(value, raw)),
        Err(W2nError::Words2Num(_)) => {}
        Err(e) => return Err(e),
    }

    match call_words2num(text, lang) {
        Ok(v) => Ok(Quantity::bare(v, raw)),
        // raise Words2NumError("could not auto-parse %r: %s" % (raw, exc))
        Err(e) => Err(W2nError::Words2Num(format!(
            "could not auto-parse {}: {}",
            py_repr_str(raw),
            e.message()
        ))),
    }
}

// ===========================================================================
// `auto_parse_sentence`
// ===========================================================================

/// `NUM = r"[-+]?\d+(?:[.,'_\xa0   ]\d+)*"`
///
/// Note this is stricter than the `[\d.,'… _]+` class used inside
/// `auto_parse`: a separator only counts when digits immediately follow, which
/// is what keeps the regex from swallowing the `", "` after `"$12.50"`.
fn eat_num(cur: &mut Cursor) -> Option<String> {
    let start = cur.i;
    cur.eat_if(|c| c == '-' || c == '+');
    if !cur.peek().is_some_and(is_unicode_digit) {
        cur.i = start;
        return None;
    }
    cur.take_while(is_unicode_digit);
    // (?:sep \d+)* — greedy; each round needs digits after the separator.
    loop {
        let save = cur.i;
        if !cur.eat_if(is_num_sep) {
            break;
        }
        if !cur.peek().is_some_and(is_unicode_digit) {
            cur.i = save;
            break;
        }
        cur.take_while(is_unicode_digit);
    }
    Some(cur.c[start..cur.i].iter().collect())
}

/// One attempt of `auto_parse_sentence`'s pattern at `pos`; returns the end
/// index of the match.
///
/// ```text
/// (?: [$€£¥₹₽₩₺]\s* NUM (?:\s*[kKmMbBtT][nN]?)?
///   | NUM (?:\s*(?: °[CF]? | % | [$€£¥₹₽₩₺] | [A-Z]{3} | [a-zA-Z µ]+ ))?
/// )
/// ```
/// Alternation is ordered, so `[A-Z]{3}` wins over `[a-zA-Z]+` — which is why
/// `"45USDX"` matches only `"45USD"` and leaves the `X` behind.
fn match_quantity_at(chars: &[char], pos: usize) -> Option<usize> {
    // Alternative 1 — currency symbol prefix.
    {
        let mut cur = Cursor {
            c: chars,
            i: pos,
        };
        if cur.peek().is_some_and(is_currency_symbol) {
            cur.i += 1;
            cur.skip_ws();
            if eat_num(&mut cur).is_some() {
                let save = cur.i;
                cur.skip_ws();
                if cur.eat_if(|c| "kKmMbBtT".contains(c)) {
                    cur.eat_if(|c| c == 'n' || c == 'N');
                } else {
                    cur.i = save;
                }
                return Some(cur.i);
            }
        }
    }
    // Alternative 2 — number with an optional trailing unit/currency token.
    {
        let mut cur = Cursor {
            c: chars,
            i: pos,
        };
        if eat_num(&mut cur).is_some() {
            let save = cur.i;
            cur.skip_ws();
            let matched = if cur.peek() == Some('\u{b0}') {
                cur.i += 1;
                cur.eat_if(|c| c == 'C' || c == 'F');
                true
            } else if cur.peek() == Some('%') || cur.peek().is_some_and(is_currency_symbol) {
                // `% | [$€£¥₹₽₩₺]` — both consume exactly one character.
                cur.i += 1;
                true
            } else if (0..3).all(|k| cur.at(k).is_some_and(|c| c.is_ascii_uppercase())) {
                cur.i += 3;
                true
            } else if cur
                .peek()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '\u{b5}')
            {
                cur.take_while(|c| c.is_ascii_alphabetic() || c == '\u{b5}');
                true
            } else {
                false
            };
            if !matched {
                cur.i = save;
            }
            return Some(cur.i);
        }
    }
    None
}

/// `converters.auto._format_number`
///
/// ```python
/// if isinstance(value, float) and value.is_integer(): return str(int(value))
/// return str(value)
/// ```
fn format_number(value: &W2nValue) -> String {
    if let W2nValue::Float(f) = value {
        // float.is_integer() is False for inf/nan.
        if f.is_finite() && f.fract() == 0.0 {
            if *f == 0.0 {
                return "0".to_string(); // int(-0.0) == 0
            }
            if let Some(i) = BigInt::from_f64(*f) {
                return i.to_string();
            }
        }
    }
    value.py_str()
}

/// `converters.auto._IRREGULAR_PLURALS`
fn irregular_plural(long_form: &str) -> Option<&'static str> {
    Some(match long_form {
        "foot" => "feet",
        "inch" => "inches",
        "pound sterling" => "pounds sterling",
        "degree celsius" => "degrees celsius",
        "degree fahrenheit" => "degrees fahrenheit",
        "Swiss franc" => "Swiss francs",
        "Canadian dollar" => "Canadian dollars",
        "Australian dollar" => "Australian dollars",
        "Mexican peso" => "Mexican pesos",
        "US dollar" => "US dollars",
        _ => return None,
    })
}

/// `converters.auto._UNCOUNTABLE`
fn is_uncountable(long_form: &str) -> bool {
    matches!(long_form, "yen" | "yuan" | "won" | "kelvin" | "percent")
}

/// Port of `converters.auto.pluralize`.
pub fn pluralize(long_form: Option<&str>, value: &W2nValue) -> Option<String> {
    let long_form = long_form?;
    // `if value in (1, -1, 1.0, -1.0)` — membership compares with ==.
    if value.is_unit_magnitude() {
        return Some(long_form.to_string());
    }
    if is_uncountable(long_form) {
        return Some(long_form.to_string());
    }
    if let Some(p) = irregular_plural(long_form) {
        return Some(p.to_string());
    }
    if long_form.ends_with('s')
        || long_form.ends_with('x')
        || long_form.ends_with('z')
        || long_form.ends_with("ch")
        || long_form.ends_with("sh")
    {
        return Some(format!("{}es", long_form));
    }
    let chars: Vec<char> = long_form.chars().collect();
    if chars.last() == Some(&'y') && chars.len() >= 2 && !"aeiou".contains(chars[chars.len() - 2]) {
        let stem: String = chars[..chars.len() - 1].iter().collect();
        return Some(format!("{}ies", stem));
    }
    Some(format!("{}s", long_form))
}

/// `converters.auto._format_quantity`
fn format_quantity(q: &Quantity, expand: bool) -> String {
    let Some(unit) = q.unit.as_deref() else {
        return format_number(&q.value);
    };
    if expand {
        let label = pluralize(q.unit_long.as_deref(), &q.value);
        // "{} {}".format(x, None) renders the literal "None".
        return format!(
            "{} {}",
            format_number(&q.value),
            label.unwrap_or_else(|| "None".to_string())
        );
    }
    // Short form: tight glue for percent and bare degree.
    if unit == "%" || unit == "\u{b0}" {
        return format!("{}{}", format_number(&q.value), unit);
    }
    format!("{} {}", format_number(&q.value), unit)
}

/// Port of `converters.auto.auto_parse_sentence`.
///
/// Walks free text and rewrites every quantity expression in place. A match
/// that `auto_parse` rejects with `Words2NumError` is left verbatim — but a
/// `KeyError` (see [`W2nError::Key`]) propagates, exactly as in Python, where
/// `_replace` only catches `Words2NumError`.
pub fn auto_parse_sentence(
    text: &str,
    lang: &str,
    prefer: &HashMap<String, String>,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
    expand: bool,
) -> Result<String, W2nError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        match match_quantity_at(&chars, i) {
            // The pattern can never match empty (both alternatives need at
            // least one character), so no zero-width-match handling is needed.
            Some(end) if end > i => {
                let raw: String = chars[i..end].iter().collect();
                match auto_parse(&raw, lang, prefer, thousands_sep, decimal_sep) {
                    Ok(q) => out.push_str(&format_quantity(&q, expand)),
                    Err(W2nError::Words2Num(_)) => out.push_str(&raw),
                    Err(e) => return Err(e),
                }
                i = end;
            }
            _ => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> W2nValue {
        W2nValue::Int(BigInt::from(n))
    }

    #[test]
    fn resolve_lang_rules() {
        assert_eq!(resolve_lang("en").unwrap(), "en");
        assert_eq!(resolve_lang("en-US").unwrap(), "en"); // dash + prefix
        assert_eq!(resolve_lang("zh").unwrap(), "zh");
        assert!(resolve_lang("xx").is_err());
    }

    #[test]
    fn converter_en_cardinal_ordinal_year() {
        let c = converter_for("en");
        assert_eq!(c.to_cardinal("forty-two").unwrap(), int(42));
        assert_eq!(c.to_ordinal("twenty-first").unwrap(), int(21));
        assert_eq!(c.to_year("nineteen ninety nine").unwrap(), int(1999));
        assert!(c.to_cardinal("minus").is_err()); // not a run head
    }

    #[test]
    fn walker_matches_python() {
        assert_eq!(
            words2num_sentence("I bought twenty-three apples.", "en", "cardinal", false).unwrap(),
            "I bought 23 apples."
        );
        assert_eq!(
            words2num_sentence(
                "In nineteen ninety nine, two thousand people came.",
                "en",
                "year",
                false
            )
            .unwrap(),
            "In 1999, 2000 people came."
        );
        // A run may not start with "minus".
        assert_eq!(
            words2num_sentence("minus forty two", "en", "cardinal", false).unwrap(),
            "minus 42"
        );
        // "point" is a valid head.
        assert_eq!(
            words2num_sentence("point five", "en", "cardinal", false).unwrap(),
            "0.5"
        );
    }

    #[test]
    fn auto_parse_currency_and_units() {
        let prefer = HashMap::new();
        let q = auto_parse("$12.50", "en", &prefer, None, None).unwrap();
        assert_eq!(q.value, W2nValue::Float(12.5));
        assert_eq!(q.unit.as_deref(), Some("USD"));
        assert_eq!(q.kind.as_deref(), Some("currency"));

        let q = auto_parse("forty-two kg", "en", &prefer, None, None).unwrap();
        assert_eq!(q.value, int(42));
        assert_eq!(q.unit.as_deref(), Some("kg"));

        let q = auto_parse("$5bn", "en", &prefer, None, None).unwrap();
        assert_eq!(q.value, int(5_000_000_000));

        // KeyError escapes for an invalid scale suffix.
        assert_eq!(
            auto_parse("$5kn", "en", &prefer, None, None),
            Err(W2nError::Key("kn".to_string()))
        );
    }

    #[test]
    fn auto_parse_sentence_rewrites() {
        let prefer = HashMap::new();
        let out = auto_parse_sentence(
            "The package weighs 5kg and costs $12.50.",
            "en",
            &prefer,
            None,
            None,
            false,
        )
        .unwrap();
        assert!(out.contains("5 kg"));
        assert!(out.contains("12.5 USD"));
    }

    #[test]
    fn pluralize_rules() {
        assert_eq!(pluralize(Some("dollar"), &int(5)).as_deref(), Some("dollars"));
        assert_eq!(pluralize(Some("dollar"), &int(1)).as_deref(), Some("dollar"));
        assert_eq!(pluralize(Some("foot"), &int(5)).as_deref(), Some("feet"));
        assert_eq!(pluralize(Some("yen"), &int(5)).as_deref(), Some("yen"));
    }
}

#[cfg(test)]
mod digits_tests {
    use super::words2num_sentence as s;
    #[test]
    fn digit_sequences_concatenate_not_sum() {
        // Spoken digit-by-digit (house numbers, zips, phone-style) -> concat.
        assert_eq!(s("Two seven five", "en", "digits", false).unwrap(), "275");
        assert_eq!(
            s("Seven Eight Three One Eight Avenue", "en", "digits", false).unwrap(),
            "78318 Avenue"
        );
        assert_eq!(
            s("one eight nine zero zero one one", "en", "digits", false).unwrap(),
            "1890011"
        );
        // Multilingual via the reverse tables.
        assert_eq!(s("sieben acht drei", "de", "digits", false).unwrap(), "783");
        // Cardinals are NOT digit runs -> declined, left verbatim in digits mode
        // (handled by the cardinal converter instead), and never mis-summed.
        assert_eq!(
            s("sixty-one hundred Main Street", "en", "digits", false).unwrap(),
            "sixty-one hundred Main Street"
        );
        // The old cardinal behaviour that motivated this: "two seven five" -> 14.
        assert_eq!(s("Two seven five", "en", "cardinal", false).unwrap(), "14");
    }
}
