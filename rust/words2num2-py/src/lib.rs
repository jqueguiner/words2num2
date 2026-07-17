//! Rust core for words2num2 — the inverse of num2words2.
//!
//! # Why this is small
//!
//! 119 of words2num2's 120 locales never had a hand-written parser. They use
//! `Words2Num_Base`, which materialises a reverse lookup table by calling
//! `num2words` across `LOOKUP_RANGE` (`range(-1, 10001)`) — 10,002 renders —
//! and then does a dict hit. Only `en` is hand-written.
//!
//! So the port is: the generic table backend + `_normalize` + the `en`
//! grammar parser. The table is now built by calling the Rust num2words core
//! directly, which is where the speedup comes from.

use num2words2_core::base::Lang;
use num_bigint::BigInt;
use pyo3::exceptions::{PyNotImplementedError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::IntoPyObjectExt;
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

// The three agent-written ports. Each is self-contained and keeps its own
// module-private `W2nValue` / error type; they never reference one another's
// types, so no shared value type is needed. The only cross-module dependency
// is `w2n_lang_en` reaching up to this crate root's `normalize_py` /
// `normalize_tail` (private items are visible to descendant modules). The
// unification happens purely at this PyO3 boundary, where the wrappers below
// convert each module's native value/error into Python objects.
pub mod w2n_formats;
pub mod w2n_lang_en;
pub mod w2n_sentence;

/// Python's `Words2Num_Base.LOOKUP_RANGE`.
const LOOKUP_LO: i64 = -1;
const LOOKUP_HI: i64 = 10001;

/// Port of `Words2Num_Base._normalize`.
///
/// ```python
/// nfkd = unicodedata.normalize("NFKD", text)
/// text = "".join(c for c in nfkd if not unicodedata.combining(c))
/// text = text.lower().replace("_", " ")
/// text = re.sub(r"(?<=[a-z])-(?=[a-z])", " ", text)
/// text = re.sub(r"[,;:!\?\"']", " ", text)
/// text = re.sub(r"\.(?!\d)", " ", text)
/// text = re.sub(r"\s+", " ", text).strip()
/// ```
///
/// The NFKD decomposition + combining-mark strip is what makes "trente-deux"
/// match "trente deux"; it is done on the Python side (see `normalize_py`)
/// because reimplementing Unicode normalisation here would be a second
/// source of truth for no gain. Everything after it is pure ASCII-shaped
/// rewriting and lives here.
fn normalize_tail(decomposed: &str) -> String {
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

/// The reverse tables, built lazily per (lang, kind) exactly as Python does.
fn tables() -> &'static RwLock<HashMap<(String, bool), HashMap<String, i64>>> {
    static T: OnceLock<RwLock<HashMap<(String, bool), HashMap<String, i64>>>> = OnceLock::new();
    T.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Port of `Words2Num_Base._build_table`.
///
/// Python calls `num2words(n, lang, to=kind)` for every n in LOOKUP_RANGE and
/// does `table.setdefault(key, n)` — **first write wins**, so the canonical
/// short form takes precedence over later spellings. That ordering is
/// load-bearing; do not switch to insert-overwrite.
fn build_table(py: Python<'_>, lang: &str, ordinal: bool) -> PyResult<HashMap<String, i64>> {
    let l = num2words2_core::get_lang_by_key(lang)
        .ok_or_else(|| PyNotImplementedError::new_err(lang.to_string()))?;
    let mut table = HashMap::new();
    for n in LOOKUP_LO..LOOKUP_HI {
        let v = BigInt::from(n);
        let words = if ordinal { l.to_ordinal(&v) } else { l.to_cardinal(&v) };
        let Ok(words) = words else { continue }; // Python swallows every raise
        let key = normalize_py(py, &words)?;
        table.entry(key).or_insert(n);
    }
    Ok(table)
}

/// `unicodedata.normalize("NFKD", s)` with combining marks stripped, then the
/// ASCII-shaped tail. Unicode normalisation is delegated to Python's own
/// `unicodedata` so the two sides cannot disagree about it.
fn normalize_py(py: Python<'_>, s: &str) -> PyResult<String> {
    let ud = py.import("unicodedata")?;
    let nfkd: String = ud.call_method1("normalize", ("NFKD", s))?.extract()?;
    let mut stripped = String::with_capacity(nfkd.len());
    for c in nfkd.chars() {
        let combining: i32 = ud.call_method1("combining", (c.to_string(),))?.extract()?;
        if combining == 0 {
            stripped.push(c);
        }
    }
    Ok(normalize_tail(&stripped))
}

#[pyfunction]
fn normalize(py: Python<'_>, text: &str) -> PyResult<String> {
    normalize_py(py, text)
}

/// Port of `Words2Num_Base._lookup` + `to_cardinal`/`to_ordinal`.
///
/// Returns `None` when the text is not in the table, so the Python side can
/// raise `Words2NumError` with its exact message rather than us guessing it.
#[pyfunction]
#[pyo3(signature = (lang, text, ordinal, negative_words))]
fn lookup(
    py: Python<'_>,
    lang: &str,
    text: &str,
    ordinal: bool,
    negative_words: Vec<String>,
) -> PyResult<Option<i64>> {
    let mut normalized = normalize_py(py, text)?;
    if normalized.is_empty() {
        return Ok(None);
    }

    let mut sign = 1i64;
    for neg in &negative_words {
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
    let built = build_table(py, lang, ordinal)?;
    let got = built.get(&normalized).map(|v| sign * v);
    tables().write().unwrap().insert(key, built);
    Ok(got)
}

/// Languages the Rust core can serve.
#[pyfunction]
fn supported_langs() -> Vec<&'static str> {
    num2words2_core::supported_lang_keys()
}

#[pyfunction]
fn parse_int(s: &str) -> PyResult<i64> {
    s.parse::<i64>().map_err(|e| PyValueError::new_err(e.to_string()))
}

// ---------------------------------------------------------------------------
// Wiring for the three ported modules
// ---------------------------------------------------------------------------

/// Raise `words2num2.base.Words2NumError` (a `ValueError` subclass) carrying
/// `msg`, so callers' `except Words2NumError` keeps working. Falls back to a
/// plain `ValueError` if the Python class cannot be imported.
fn words2num_error(py: Python<'_>, msg: String) -> PyErr {
    match py
        .import("words2num2.base")
        .and_then(|b| b.getattr("Words2NumError"))
    {
        Ok(cls) => match cls.call1((msg.clone(),)) {
            Ok(v) => PyErr::from_value(v),
            Err(e) => e,
        },
        Err(_) => PyValueError::new_err(msg),
    }
}

/// `decimal.Decimal(s)` — rebuilds an exact `Decimal` from its `str()` form.
fn make_decimal(py: Python<'_>, s: String) -> PyResult<PyObject> {
    let decimal = py.import("decimal")?.getattr("Decimal")?;
    Ok(decimal.call1((s,))?.unbind())
}

/// Convert a `w2n_lang_en::W2nValue` (`int` / `float` / `Decimal`) to Python.
/// `PyDec`'s `Display` is Python's `str(Decimal)`, so `Decimal(pydec.to_string())`
/// reproduces the value (including a signed zero) exactly.
fn en_value_to_py(py: Python<'_>, v: w2n_lang_en::W2nValue) -> PyResult<PyObject> {
    use w2n_lang_en::W2nValue;
    match v {
        W2nValue::Int(i) => i.into_py_any(py),
        W2nValue::Float(f) => f.into_py_any(py),
        W2nValue::Dec(d) => make_decimal(py, d.to_string()),
    }
}

/// Convert a `w2n_formats::W2nValue` to Python. `parse_number_string` only ever
/// yields `int` / `float`; the `Decimal` arm is here for completeness.
fn formats_value_to_py(py: Python<'_>, v: w2n_formats::W2nValue) -> PyResult<PyObject> {
    use w2n_formats::W2nValue;
    match v {
        W2nValue::Int(i) => i.into_py_any(py),
        W2nValue::Float(f) => f.into_py_any(py),
        W2nValue::Dec(d) => make_decimal(py, d.to_string()),
    }
}

/// `Words2Num_EN().to_cardinal(text)`.
#[pyfunction]
fn en_to_cardinal(py: Python<'_>, text: &str) -> PyResult<PyObject> {
    match w2n_lang_en::W2nLangEn::new().to_cardinal(text) {
        Ok(v) => en_value_to_py(py, v),
        Err(e) => Err(words2num_error(py, e.msg)),
    }
}

/// `Words2Num_EN().to_ordinal(text)`.
#[pyfunction]
fn en_to_ordinal(py: Python<'_>, text: &str) -> PyResult<PyObject> {
    match w2n_lang_en::W2nLangEn::new().to_ordinal(text) {
        Ok(v) => en_value_to_py(py, v),
        Err(e) => Err(words2num_error(py, e.msg)),
    }
}

/// `Words2Num_EN().to_year(text)`.
#[pyfunction]
fn en_to_year(py: Python<'_>, text: &str) -> PyResult<PyObject> {
    match w2n_lang_en::W2nLangEn::new().to_year(text) {
        Ok(v) => en_value_to_py(py, v),
        Err(e) => Err(words2num_error(py, e.msg)),
    }
}

/// `words2num2.formats.parse_number_string(s, thousands_sep, decimal_sep, lang)`.
#[pyfunction]
#[pyo3(signature = (s, thousands_sep=None, decimal_sep=None, lang=None))]
fn parse_number_string(
    py: Python<'_>,
    s: &str,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
    lang: Option<&str>,
) -> PyResult<PyObject> {
    match w2n_formats::parse_number_string(s, thousands_sep, decimal_sep, lang) {
        Ok(v) => formats_value_to_py(py, v),
        Err(e) => Err(words2num_error(py, e.0)),
    }
}

/// `words2num2.words2num_sentence(sentence, lang, to, **kwargs)`.
#[pyfunction]
#[pyo3(signature = (sentence, lang="en", to="cardinal", kwargs=None))]
fn words2num_sentence(
    py: Python<'_>,
    sentence: &str,
    lang: &str,
    to: &str,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    w2n_sentence::words2num_sentence(py, sentence, lang, to, kwargs.as_ref())
        .map_err(|e| e.into_pyerr(py))
}

/// Convert a `w2n_sentence::W2nValue` (`int` / `float` / `Decimal`) to Python.
fn sentence_value_to_py(py: Python<'_>, v: &w2n_sentence::W2nValue) -> PyResult<PyObject> {
    use w2n_sentence::W2nValue;
    match v {
        W2nValue::Int(i) => i.clone().into_py_any(py),
        W2nValue::Float(f) => (*f).into_py_any(py),
        W2nValue::Dec(_) => make_decimal(py, v.py_str()),
    }
}

/// Build a `words2num2.converters.auto.Quantity` from the Rust struct. The
/// dataclass stays defined in Python (it is a public API type users import);
/// only the parsing logic moves to Rust.
fn quantity_to_py(py: Python<'_>, q: w2n_sentence::Quantity) -> PyResult<PyObject> {
    let value = sentence_value_to_py(py, &q.value)?;
    let cls = py
        .import("words2num2.converters.auto")?
        .getattr("Quantity")?;
    let kwargs = PyDict::new(py);
    kwargs.set_item("value", value)?;
    kwargs.set_item("unit", q.unit)?;
    kwargs.set_item("unit_long", q.unit_long)?;
    kwargs.set_item("kind", q.kind)?;
    kwargs.set_item("confidence", q.confidence)?;
    kwargs.set_item("raw", q.raw)?;
    Ok(cls.call((), Some(&kwargs))?.unbind())
}

/// `words2num2.converters.auto.pluralize(long_form, value)`. Exposed so the
/// standalone helper and `auto_parse_sentence`'s internal use share one
/// implementation (English long-form unit pluralisation).
#[pyfunction]
#[pyo3(signature = (long_form, value))]
fn pluralize(long_form: Option<&str>, value: Bound<'_, PyAny>) -> PyResult<Option<String>> {
    let v = w2n_sentence::W2nValue::from_py(&value)?;
    Ok(w2n_sentence::pluralize(long_form, &v))
}

/// `words2num2.auto_parse(text, lang, prefer, thousands_sep, decimal_sep)`.
#[pyfunction]
#[pyo3(signature = (text, lang="en", prefer=None, thousands_sep=None, decimal_sep=None))]
fn auto_parse(
    py: Python<'_>,
    text: &str,
    lang: &str,
    prefer: Option<HashMap<String, String>>,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
) -> PyResult<PyObject> {
    let prefer = prefer.unwrap_or_default();
    match w2n_sentence::auto_parse(py, text, lang, &prefer, thousands_sep, decimal_sep) {
        Ok(q) => quantity_to_py(py, q),
        Err(e) => Err(e.into_pyerr(py)),
    }
}

/// `words2num2.auto_parse_sentence(text, lang, prefer, thousands_sep,
/// decimal_sep, expand)`.
#[pyfunction]
#[pyo3(signature = (text, lang="en", prefer=None, thousands_sep=None, decimal_sep=None, expand=false))]
fn auto_parse_sentence(
    py: Python<'_>,
    text: &str,
    lang: &str,
    prefer: Option<HashMap<String, String>>,
    thousands_sep: Option<&str>,
    decimal_sep: Option<&str>,
    expand: bool,
) -> PyResult<String> {
    let prefer = prefer.unwrap_or_default();
    w2n_sentence::auto_parse_sentence(py, text, lang, &prefer, thousands_sep, decimal_sep, expand)
        .map_err(|e| e.into_pyerr(py))
}

#[pymodule]
fn _rust(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(normalize, m)?)?;
    m.add_function(wrap_pyfunction!(lookup, m)?)?;
    m.add_function(wrap_pyfunction!(supported_langs, m)?)?;
    m.add_function(wrap_pyfunction!(parse_int, m)?)?;
    m.add_function(wrap_pyfunction!(en_to_cardinal, m)?)?;
    m.add_function(wrap_pyfunction!(en_to_ordinal, m)?)?;
    m.add_function(wrap_pyfunction!(en_to_year, m)?)?;
    m.add_function(wrap_pyfunction!(parse_number_string, m)?)?;
    m.add_function(wrap_pyfunction!(words2num_sentence, m)?)?;
    m.add_function(wrap_pyfunction!(pluralize, m)?)?;
    m.add_function(wrap_pyfunction!(auto_parse, m)?)?;
    m.add_function(wrap_pyfunction!(auto_parse_sentence, m)?)?;
    Ok(())
}
