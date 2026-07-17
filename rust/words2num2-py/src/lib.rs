//! PyO3 binder for words2num2 — a thin shim over the pure-Rust
//! [`words2num2_core`] engine.
//!
//! All parsing logic (the reverse-table backend, the English grammar, the
//! sentence walker, `auto_parse`, number-format handling) lives in
//! `words2num2-core`, which has no Python dependency. This crate only:
//!   * exposes the same `#[pyfunction]` set the Python package calls, and
//!   * converts core native values / errors to and from Python objects.

use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use pyo3::exceptions::{PyKeyError, PyNotImplementedError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::IntoPyObjectExt;
use std::collections::HashMap;
use std::str::FromStr;

use words2num2_core::w2n_formats;
use words2num2_core::w2n_lang_en;
use words2num2_core::w2n_sentence;

// ---------------------------------------------------------------------------
// Python object / error helpers
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

/// Map a core sentence-layer error onto the matching Python exception class.
fn w2n_error_to_pyerr(py: Python<'_>, e: w2n_sentence::W2nError) -> PyErr {
    match e {
        w2n_sentence::W2nError::Words2Num(m) => words2num_error(py, m),
        w2n_sentence::W2nError::NotImplemented(m) => PyNotImplementedError::new_err(m),
        w2n_sentence::W2nError::Key(k) => PyKeyError::new_err(k),
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

/// Convert a `w2n_sentence::W2nValue` (`int` / `float` / `Decimal`) to Python.
fn sentence_value_to_py(py: Python<'_>, v: &w2n_sentence::W2nValue) -> PyResult<PyObject> {
    use w2n_sentence::W2nValue;
    match v {
        W2nValue::Int(i) => i.clone().into_py_any(py),
        W2nValue::Float(f) => (*f).into_py_any(py),
        W2nValue::Dec(_) => make_decimal(py, v.py_str()),
    }
}

/// Build a core `w2n_sentence::W2nValue` from a Python `int` / `float` /
/// `Decimal` (used by `pluralize`).
fn w2nvalue_from_py(obj: &Bound<'_, PyAny>) -> PyResult<w2n_sentence::W2nValue> {
    use pyo3::types::{PyFloat, PyInt};
    if obj.is_instance_of::<PyInt>() {
        return Ok(w2n_sentence::W2nValue::Int(obj.extract::<BigInt>()?));
    }
    if obj.is_instance_of::<PyFloat>() {
        return Ok(w2n_sentence::W2nValue::Float(obj.extract::<f64>()?));
    }
    // Decimal — go through its own `str()` so the (coefficient, exponent)
    // pair survives exactly.
    let s: String = obj.str()?.extract()?;
    BigDecimal::from_str(&s)
        .map(w2n_sentence::W2nValue::Dec)
        .map_err(|_| PyTypeError::new_err(format!("unsupported value {}", s)))
}

/// Build a `words2num2.converters.auto.Quantity` from the core struct. The
/// dataclass stays defined in Python (it is a public API type users import);
/// only the parsing logic lives in the core.
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

// ---------------------------------------------------------------------------
// #[pyfunction] surface — identical names/signatures to the historic shim
// ---------------------------------------------------------------------------

/// `_rust.normalize(text)` — `Words2Num_Base._normalize`.
#[pyfunction]
fn normalize(text: &str) -> String {
    words2num2_core::normalize(text)
}

/// `_rust.lookup(lang, text, ordinal, negative_words)`.
///
/// Returns `None` when the text is not in the reverse table.
#[pyfunction]
#[pyo3(signature = (lang, text, ordinal, negative_words))]
fn lookup(
    lang: &str,
    text: &str,
    ordinal: bool,
    negative_words: Vec<String>,
) -> PyResult<Option<i64>> {
    words2num2_core::lookup(lang, text, ordinal, &negative_words).map_err(|e| match e {
        words2num2_core::LookupError::NotImplemented(l) => PyNotImplementedError::new_err(l),
    })
}

/// `_rust.supported_langs()`.
#[pyfunction]
fn supported_langs() -> Vec<&'static str> {
    words2num2_core::supported_langs()
}

/// `_rust.parse_int(s)`.
#[pyfunction]
fn parse_int(s: &str) -> PyResult<i64> {
    words2num2_core::parse_int(s).map_err(|e| PyValueError::new_err(e.to_string()))
}

/// `_rust.en_to_cardinal(text)`.
#[pyfunction]
fn en_to_cardinal(py: Python<'_>, text: &str) -> PyResult<PyObject> {
    match words2num2_core::en_to_cardinal(text) {
        Ok(v) => en_value_to_py(py, v),
        Err(e) => Err(words2num_error(py, e.msg)),
    }
}

/// `_rust.en_to_ordinal(text)`.
#[pyfunction]
fn en_to_ordinal(py: Python<'_>, text: &str) -> PyResult<PyObject> {
    match words2num2_core::en_to_ordinal(text) {
        Ok(v) => en_value_to_py(py, v),
        Err(e) => Err(words2num_error(py, e.msg)),
    }
}

/// `_rust.en_to_year(text)`.
#[pyfunction]
fn en_to_year(py: Python<'_>, text: &str) -> PyResult<PyObject> {
    match words2num2_core::en_to_year(text) {
        Ok(v) => en_value_to_py(py, v),
        Err(e) => Err(words2num_error(py, e.msg)),
    }
}

/// `_rust.parse_number_string(s, thousands_sep, decimal_sep, lang)`.
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

/// `_rust.words2num_sentence(sentence, lang, to, kwargs)`.
#[pyfunction]
#[pyo3(signature = (sentence, lang="en", to="cardinal", kwargs=None))]
fn words2num_sentence(
    py: Python<'_>,
    sentence: &str,
    lang: &str,
    to: &str,
    kwargs: Option<Bound<'_, PyDict>>,
) -> PyResult<String> {
    // Python passed `kwargs or None`, so a present dict is always non-empty;
    // the guard keeps parity if an empty dict is ever passed directly.
    let has_kwargs = kwargs.as_ref().is_some_and(|d| !d.is_empty());
    w2n_sentence::words2num_sentence(sentence, lang, to, has_kwargs)
        .map_err(|e| w2n_error_to_pyerr(py, e))
}

/// `_rust.pluralize(long_form, value)`.
#[pyfunction]
#[pyo3(signature = (long_form, value))]
fn pluralize(long_form: Option<&str>, value: Bound<'_, PyAny>) -> PyResult<Option<String>> {
    let v = w2nvalue_from_py(&value)?;
    Ok(w2n_sentence::pluralize(long_form, &v))
}

/// `_rust.auto_parse(text, lang, prefer, thousands_sep, decimal_sep)`.
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
    match w2n_sentence::auto_parse(text, lang, &prefer, thousands_sep, decimal_sep) {
        Ok(q) => quantity_to_py(py, q),
        Err(e) => Err(w2n_error_to_pyerr(py, e)),
    }
}

/// `_rust.auto_parse_sentence(text, lang, prefer, thousands_sep, decimal_sep,
/// expand)`.
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
    w2n_sentence::auto_parse_sentence(text, lang, &prefer, thousands_sep, decimal_sep, expand)
        .map_err(|e| w2n_error_to_pyerr(py, e))
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
