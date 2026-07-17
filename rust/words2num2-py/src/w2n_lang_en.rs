//! Port of `words2num2/words2num2/lang_EN.py` — the `Words2Num_EN` grammar.
//!
//! English is the only one of words2num2's 120 locales with a hand-written
//! parser; the other 119 ride `Words2Num_Base`'s reverse lookup table, which
//! `lib.rs` already ports. So this file is `_parse` / `_cardinal_value` /
//! `_fractional_value` / `_looks_like_year` / `_year_value` and their tables,
//! and nothing else.
//!
//! # Fidelity notes — read before "fixing" anything here
//!
//! Python is the specification, bugs included. Four of its behaviours look
//! like mistakes and are reproduced deliberately:
//!
//! * **`_parse` rewrites a trailing ordinal regardless of the `ordinal`
//!   flag.** `to_cardinal("twenty first")` is 21, and `to_ordinal("forty
//!   two")` is 42. The `if ordinal and not was_ordinal:` branch in Python is
//!   a bare `pass`; it is preserved below as a comment so the shape matches.
//! * **`_cardinal_value`'s `"no number tokens in input"` is dead code.**
//!   Every loop branch sets `seen_any = True` or raises, and the empty list
//!   is rejected up front, so `seen_any` can never be false at the check.
//!   Ported anyway — it costs nothing and keeps the port line-for-line.
//! * **`_year_value` falls back to plain addition.** `to_year("nine
//!   eleven")` is 20, not 911: `high = 9` fails the `10 <= high` test, no
//!   split matches, and the fallback `_cardinal_value` adds 9 + 11.
//! * **`_looks_like_year` never checks that the halves are pair-shaped**
//!   despite its docstring; it only bounds the token count and rejects
//!   "hundred"/"thousand".
//!
//! # Three places where a naive port is silently wrong
//!
//! 1. **`Decimal` arithmetic is context-bound.** Python's default context is
//!    `prec=28, ROUND_HALF_EVEN`. `Decimal(int_part) + frac_part` and the
//!    `sign * ...` that follows both round to 28 significant digits.
//!    `BigDecimal` is arbitrary-precision and would *not*, so the decimal
//!    path here runs on [`PyDec`], a direct model of Python's
//!    `(sign, coefficient, exponent)` triple with `_fix`/`_normalize`/
//!    `_rescale` ported. Live proof, from the oracle:
//!    `"nine hundred ninety nine centillion point one two three"` →
//!    `Decimal('9.990000000000000000000000000E+305')` — the fraction is
//!    rounded clean away.
//! 2. **`Decimal` has a signed zero; `BigDecimal` does not.**
//!    `"minus zero point zero"` → `Decimal('-0.0')`, and `-0.0` is a
//!    distinct object from `0.0` (`repr` differs, `is_signed()` is True).
//!    [`PyDec`] carries the sign separately so this survives; see the note on
//!    [`W2nValue::Dec`] about the deviation from a plain `Dec(BigDecimal)`.
//! 3. **Python's `\d` is Unicode Nd, not `[0-9]`,** and `int()`/`float()`
//!    accept those digits. `to_cardinal("٤٢")` really is 42, and
//!    `"two thousand ٤"` really is 2004 (via `_cardinal_value`'s embedded
//!    digit-group branch). See [`ND_RUNS`].
//!
//! Note also that this crate builds with `panic = "abort"`, so a panic would
//! take the host interpreter down with it. Nothing here unwraps or slices on
//! an unproven bound.

use bigdecimal::BigDecimal;
use num_bigint::{BigInt, Sign};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Port of `words2num2.base.Words2NumError`.
///
/// Python's is a `ValueError` subclass; the message is reproduced byte for
/// byte, including the `%r` (Python `repr`) formatting of offending tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct W2nError {
    pub msg: String,
}

impl W2nError {
    fn new(msg: impl Into<String>) -> Self {
        W2nError { msg: msg.into() }
    }
}

impl fmt::Display for W2nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for W2nError {}

pub type W2nResult<T> = Result<T, W2nError>;

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// What `Words2Num_EN` can return: Python's `int`, `float` or `Decimal`.
///
/// `Dec` holds a [`PyDec`] rather than the `BigDecimal` the port brief
/// suggested. `BigDecimal` cannot express two things Python's `Decimal`
/// does and this module produces:
///
/// * `Decimal('-0.0')` — `BigDecimal` has no signed zero (`-0.0` parses to
///   `sign=NoSign`), so `"minus zero point zero"` would come back `0.0`.
/// * The *value* of a coefficient rounded to `prec=28` (`BigDecimal` add is
///   exact) and the exact `str()` spelling, e.g. `9.99…E+305`, which
///   `BigDecimal`'s `Display` renders as `999e+303`.
///
/// [`PyDec::to_bigdecimal`] converts when the sign of zero is not needed, and
/// [`PyDec`]'s `Display` is Python's `str(Decimal)`, so the wiring step can
/// rebuild an exact `Decimal` with `Decimal(value.to_string())`.
#[derive(Debug, Clone, PartialEq)]
pub enum W2nValue {
    Int(BigInt),
    Float(f64),
    Dec(PyDec),
}

// ---------------------------------------------------------------------------
// Python's `\d`: Unicode category Nd
// ---------------------------------------------------------------------------

/// Start of every Unicode decimal-digit run (category `Nd`).
///
/// `re`'s `\d` on `str` matches `Nd`, and `int()`/`float()` accept those
/// digits, so `_parse`'s digit short-circuits fire on non-ASCII input:
/// `to_cardinal("٤٢") == 42`. Every `Nd` character lives in a contiguous run
/// of ten starting at a "digit zero"; the oracle's CPython (Unicode 13.0.0)
/// has 65 such runs covering all 650 `Nd` characters, verified exhaustively
/// against `unicodedata`.
///
/// This is the one table pinned to a Unicode version. A CPython built on a
/// newer database would gain runs and this would under-match — flagged rather
/// than papered over.
const ND_RUNS: [u32; 65] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10, 0x104A0, 0x10D30, 0x11066, 0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0,
    0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x16A60, 0x16B50,
    0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E950, 0x1FBF0,
];

/// `\d` — the decimal value of `c`, or `None` if it is not category `Nd`.
fn nd_value(c: char) -> Option<u32> {
    let cp = c as u32;
    ND_RUNS
        .iter()
        .find(|&&start| cp >= start && cp < start + 10)
        .map(|&start| cp - start)
}

/// Fold Unicode `Nd` digits down to ASCII so Rust's parsers see what
/// Python's `int()`/`float()` see. Non-digits pass through untouched.
fn nd_to_ascii(s: &str) -> String {
    s.chars()
        .map(|c| match nd_value(c) {
            // `v` is 0..=9, so `from_digit` cannot fail.
            Some(v) => char::from_digit(v, 10).unwrap_or(c),
            None => c,
        })
        .collect()
}

/// `re.fullmatch(r"\d+", s)`.
fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| nd_value(c).is_some())
}

/// `re.fullmatch(r"\d", s)` — exactly one digit.
fn is_single_digit(s: &str) -> bool {
    let mut it = s.chars();
    matches!((it.next().map(nd_value), it.next()), (Some(Some(_)), None))
}

/// `re.fullmatch(r"-?\d+", s)`.
fn is_signed_int(s: &str) -> bool {
    is_digits(s.strip_prefix('-').unwrap_or(s))
}

/// `re.fullmatch(r"-?\d+\.\d+", s)`.
fn is_signed_float(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    match body.split_once('.') {
        Some((int, frac)) => is_digits(int) && is_digits(frac),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Python's repr(), for the error messages
// ---------------------------------------------------------------------------

/// `repr()` of a `str`, as `"unrecognized token %r in %r"` needs it.
///
/// Verified against the oracle: `repr` of two backslashes is `'\\\\'`, and
/// `'foo\x07bar'` keeps its `\x07` escape. `_normalize` has already stripped
/// every `\s`, `,;:!?"'` from the input, so in practice the quote is always
/// `'` and only backslashes and C0/C1 controls need escaping — but the quote
/// selection is ported anyway since it is free.
///
/// Not ported: Python escapes anything failing `str.isprintable()`, which
/// includes the `Cf`/`Co`/`Cn` categories. Those need Unicode tables and can
/// only be reached by feeding garbage to the parser; such a character passes
/// through raw here where Python would emit `\uXXXX`.
fn py_repr(s: &str) -> String {
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
            // C0 and C1 controls (category Cc), which `str.isprintable()`
            // rejects. `\s` members are unreachable post-`_normalize`.
            c if (c as u32) < 0x20 || (0x7f..0xa0).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

// ---------------------------------------------------------------------------
// PyDec — Python's decimal.Decimal under the default context
// ---------------------------------------------------------------------------

/// `getcontext().prec` — 28 by default, and load-bearing (see module docs).
const PREC: i64 = 28;

/// A `decimal.Decimal`, modelled the way CPython models it: a sign flag, a
/// non-negative integer coefficient, and a base-10 exponent.
///
/// Only the operations `_parse` performs are ported — construction from an
/// int, construction from `"0." + digits`, addition of two **non-negative**
/// values, and multiplication by `±1`. Rounding is `ROUND_HALF_EVEN` at
/// `PREC` significant digits, matching the default context.
///
/// The context's `Emax`/`Emin`/`clamp` machinery is *not* ported because this
/// grammar cannot reach it: the largest scale word is `centillion` (10^303)
/// and `_cardinal_value` **adds** scales rather than multiplying them, so a
/// coefficient of ~10^999972 (what `Etop` would require) is unreachable, and
/// a pure fraction always lands at `exp_min == -PREC`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyDec {
    /// Python's `_sign`: `false` = 0 (positive), `true` = 1 (negative).
    neg: bool,
    /// Python's `_int`, as a number. Always >= 0; leading zeros are stripped
    /// exactly as `Decimal.__new__` does (`Decimal("0.05")` has coefficient
    /// 5, exponent -2), and zero has the one-digit coefficient `0`.
    coeff: BigInt,
    /// Python's `_exp`.
    exp: i64,
}

impl PyDec {
    /// `Decimal(v)` for a non-negative int: coefficient `v`, exponent 0.
    fn from_bigint(v: &BigInt) -> PyDec {
        PyDec {
            neg: v.sign() == Sign::Minus,
            coeff: if v.sign() == Sign::Minus { -v } else { v.clone() },
            exp: 0,
        }
    }

    /// `Decimal(0)`.
    fn zero() -> PyDec {
        PyDec {
            neg: false,
            coeff: BigInt::from(0),
            exp: 0,
        }
    }

    /// `Decimal("0." + digits)` — `digits` is a non-empty ASCII digit string.
    ///
    /// `Decimal.__new__` does `_int = str(int(intpart + fracpart))`, which is
    /// why leading zeros vanish, and `_exp = -len(fracpart)`.
    fn from_frac_digits(digits: &str) -> Option<PyDec> {
        Some(PyDec {
            neg: false,
            coeff: BigInt::from_str(digits).ok()?,
            exp: -(digits.len() as i64),
        })
    }

    fn is_zero(&self) -> bool {
        self.coeff.sign() == Sign::NoSign
    }

    /// Python's `_int` — the coefficient's digit string. Zero is `"0"`.
    fn int_str(&self) -> String {
        self.coeff.to_str_radix(10)
    }

    /// `len(self._int)`.
    fn ndigits(&self) -> i64 {
        self.int_str().len() as i64
    }

    /// Port of `Decimal._fix` under the default context, minus the
    /// unreachable `Emax`/`Emin`/`clamp` branches (see the struct docs).
    ///
    /// Reduces to: if the coefficient has more than `PREC` digits, round it
    /// to `PREC` half-even and lift the exponent to match.
    fn fix(self) -> PyDec {
        if self.is_zero() {
            return self;
        }
        let int_str = self.int_str();
        let len = int_str.len() as i64;
        let mut exp_min = len + self.exp - PREC;
        if self.exp >= exp_min {
            return self; // <= PREC digits: nothing to do
        }
        // `digits = len + exp - exp_min` == PREC, and PREC > 0, so Python's
        // `if digits < 0` guard is unreachable here.
        let digits = PREC as usize;
        let changed = round_half_even(&int_str, digits);
        let mut coeff = int_str.get(..digits).unwrap_or("0").to_string();
        if coeff.is_empty() {
            coeff = "0".to_string(); // Python's `self._int[:digits] or '0'`
        }
        let mut value = BigInt::from_str(&coeff).unwrap_or_else(|_| BigInt::from(0));
        if changed > 0 {
            value += 1;
            // e.g. 28 nines -> 29 digits: drop the last and bump the exponent.
            if value.to_str_radix(10).len() as i64 > PREC {
                value /= 10;
                exp_min += 1;
            }
        }
        PyDec {
            neg: self.neg,
            coeff: value,
            exp: exp_min,
        }
    }

    /// Port of `Decimal._rescale`. Called from `__add__`'s zero branches,
    /// where the target exponent is always <= `self._exp`, so only the
    /// zero-pad path actually runs; the rounding path is ported for safety.
    fn rescale(&self, exp: i64) -> PyDec {
        if self.is_zero() {
            return PyDec {
                neg: self.neg,
                coeff: BigInt::from(0),
                exp,
            };
        }
        if self.exp >= exp {
            // pad with zeros: `self._int + '0'*(self._exp - exp)`
            let pad = (self.exp - exp) as u32;
            return PyDec {
                neg: self.neg,
                coeff: &self.coeff * BigInt::from(10).pow(pad),
                exp,
            };
        }
        let int_str = self.int_str();
        let len = int_str.len() as i64;
        let digits = len + self.exp - exp;
        if digits < 0 {
            return PyDec {
                neg: self.neg,
                coeff: BigInt::from(1),
                exp,
            };
        }
        let digits = digits as usize;
        let changed = round_half_even(&int_str, digits);
        let coeff = int_str.get(..digits).unwrap_or("0");
        let coeff = if coeff.is_empty() { "0" } else { coeff };
        let mut value = BigInt::from_str(coeff).unwrap_or_else(|_| BigInt::from(0));
        if changed == 1 {
            value += 1;
        }
        PyDec {
            neg: self.neg,
            coeff: value,
            exp,
        }
    }

    /// Port of `Decimal.__add__` for the case `_parse` produces: both
    /// operands non-negative (`_cardinal_value` and `_fractional_value` can
    /// never return a negative). The sign is applied afterwards by
    /// [`PyDec::mul_sign`], so the subtraction path is genuinely unreachable
    /// and is not ported.
    fn add(&self, other: &PyDec) -> PyDec {
        let exp = self.exp.min(other.exp);

        // Python: `if not self and not other`. Note `sign = min(s1, s2)`,
        // i.e. negative only when *both* are — this is what keeps
        // `Decimal(0) + Decimal('0.0')` positive.
        if self.is_zero() && other.is_zero() {
            return PyDec {
                neg: self.neg && other.neg,
                coeff: BigInt::from(0),
                exp,
            }
            .fix();
        }
        if self.is_zero() {
            let e = exp.max(other.exp - PREC - 1);
            return other.rescale(e).fix();
        }
        if other.is_zero() {
            let e = exp.max(self.exp - PREC - 1);
            return self.rescale(e).fix();
        }

        let (op1, op2) = normalize_pair(self, other);
        // Same sign (both non-negative): coefficients add, exponent is the
        // aligned one, sign is carried through.
        PyDec {
            neg: self.neg,
            coeff: op1.coeff + op2.coeff,
            exp: op1.exp,
        }
        .fix()
    }

    /// `sign * self` for `sign` in `{1, -1}` — the final step of `_parse`'s
    /// decimal branch.
    ///
    /// All three of `Decimal.__mul__`'s branches (zero, coefficient-is-one,
    /// general) collapse to the same thing when the other operand is `±1`:
    /// XOR the signs, keep the coefficient, keep the exponent. Because the
    /// sign is XORed rather than folded into the coefficient,
    /// `-1 * Decimal('0.0')` is `Decimal('-0.0')` — the signed zero.
    fn mul_sign(&self, neg: bool) -> PyDec {
        PyDec {
            neg: self.neg != neg,
            coeff: self.coeff.clone(),
            exp: self.exp,
        }
        .fix()
    }

    /// The value as a [`BigDecimal`].
    ///
    /// Lossy for a negative zero — `BigDecimal` has no such thing, so
    /// `Decimal('-0.0')` arrives as `0.0`. Use `to_string()` when that
    /// distinction matters.
    pub fn to_bigdecimal(&self) -> BigDecimal {
        let signed = if self.neg { -&self.coeff } else { self.coeff.clone() };
        // BigDecimal's scale is the negated exponent.
        BigDecimal::new(signed, -self.exp)
    }
}

/// Port of `Decimal.__str__` (the spec's *to-scientific-string*) with the
/// default context's `capitals=1` and `eng=False`.
///
/// This is not `BigDecimal`'s `Display`: Python prints `9.99…E+305` where
/// `BigDecimal` prints `999e+303`, and Python prints `0.0` for a zero with
/// exponent -1 where `BigDecimal` prints `0`.
impl fmt::Display for PyDec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.neg { "-" } else { "" };
        let int_str = self.int_str(); // ASCII, so byte indexing is safe
        let n = int_str.len() as i64;
        let leftdigits = self.exp + n;
        let dotplace = if self.exp <= 0 && leftdigits > -6 {
            leftdigits
        } else {
            1
        };

        let (intpart, fracpart) = if dotplace <= 0 {
            (
                "0".to_string(),
                format!(".{}{}", "0".repeat((-dotplace) as usize), int_str),
            )
        } else if dotplace >= n {
            (
                format!("{}{}", int_str, "0".repeat((dotplace - n) as usize)),
                String::new(),
            )
        } else {
            let cut = dotplace as usize;
            (
                int_str.get(..cut).unwrap_or("").to_string(),
                format!(".{}", int_str.get(cut..).unwrap_or("")),
            )
        };

        let exp = if leftdigits == dotplace {
            String::new()
        } else {
            format!("E{:+}", leftdigits - dotplace)
        };
        write!(f, "{}{}{}{}", sign, intpart, fracpart, exp)
    }
}

/// `_all_zeros(s, i)` — is everything from `i` on a zero (or nothing)?
fn all_zeros(s: &str, i: usize) -> bool {
    s.get(i..).unwrap_or("").chars().all(|c| c == '0')
}

/// `_exact_half(s, i)`.
fn exact_half(s: &str, i: usize) -> bool {
    s.as_bytes().get(i) == Some(&b'5') && all_zeros(s, i + 1)
}

/// `Decimal._round_half_up` — 1 = round up, 0 = exact, -1 = round down.
fn round_half_up(s: &str, prec: usize) -> i32 {
    match s.as_bytes().get(prec) {
        Some(d) if (b'5'..=b'9').contains(d) => 1,
        _ if all_zeros(s, prec) => 0,
        _ => -1,
    }
}

/// `Decimal._round_half_even` — the default context's rounding mode.
fn round_half_even(s: &str, prec: usize) -> i32 {
    let prev_even = prec == 0
        || matches!(
            s.as_bytes().get(prec - 1),
            Some(b'0') | Some(b'2') | Some(b'4') | Some(b'6') | Some(b'8')
        );
    if exact_half(s, prec) && prev_even {
        -1
    } else {
        round_half_up(s, prec)
    }
}

/// Port of `decimal._normalize` (the `_WorkRep` one that `__add__` calls),
/// which aligns two operands onto a common exponent.
///
/// The "avoid ridiculous computation" clause matters: when the operands are
/// wildly different magnitudes it swaps the small one for a single sticky
/// digit, which is how `999…e303 + 0.123` ends up discarding the fraction.
fn normalize_pair(op1: &PyDec, op2: &PyDec) -> (PyDec, PyDec) {
    // `tmp` is the operand with the larger exponent; Python mutates in place
    // and returns `(op1, op2)`, so the swap has to be undone on the way out.
    let swapped = op1.exp < op2.exp;
    let (mut tmp, mut other) = if swapped {
        (op2.clone(), op1.clone())
    } else {
        (op1.clone(), op2.clone())
    };

    let tmp_len = tmp.ndigits();
    let other_len = other.ndigits();
    let exp = tmp.exp + (-1).min(tmp_len - PREC - 2);
    if other_len + other.exp - 1 < exp {
        other.coeff = BigInt::from(1);
        other.exp = exp;
    }
    // `tmp.exp - other.exp` is >= 0 by construction. The cast can only fail
    // for an input with billions of fractional digits, which cannot be built.
    if let Ok(pad) = u32::try_from(tmp.exp - other.exp) {
        tmp.coeff *= BigInt::from(10).pow(pad);
    }
    tmp.exp = other.exp;

    if swapped {
        (other, tmp)
    } else {
        (tmp, other)
    }
}

// ---------------------------------------------------------------------------
// Tables — `_UNITS` / `_TENS` / `_SCALES` / `_ORDINAL_TO_CARDINAL`
// ---------------------------------------------------------------------------

/// `_UNITS`. "oh", "nought" and "naught" are all zero.
const UNITS: [(&str, i64); 23] = [
    ("zero", 0),
    ("oh", 0),
    ("nought", 0),
    ("naught", 0),
    ("one", 1),
    ("two", 2),
    ("three", 3),
    ("four", 4),
    ("five", 5),
    ("six", 6),
    ("seven", 7),
    ("eight", 8),
    ("nine", 9),
    ("ten", 10),
    ("eleven", 11),
    ("twelve", 12),
    ("thirteen", 13),
    ("fourteen", 14),
    ("fifteen", 15),
    ("sixteen", 16),
    ("seventeen", 17),
    ("eighteen", 18),
    ("nineteen", 19),
];

/// `_TENS`.
const TENS: [(&str, i64); 8] = [
    ("twenty", 20),
    ("thirty", 30),
    ("forty", 40),
    ("fifty", 50),
    ("sixty", 60),
    ("seventy", 70),
    ("eighty", 80),
    ("ninety", 90),
];

/// `_SCALES`, as (word, power-of-ten). "hundred" is 100 = 10^2.
///
/// Note "hundred" is a member here *and* is special-cased ahead of the
/// `elif tok in _SCALES` branch in `_cardinal_value`, so it never reaches the
/// scale path. Kept in the table anyway to mirror Python.
const SCALES: [(&str, u32); 23] = [
    ("hundred", 2),
    ("thousand", 3),
    ("million", 6),
    ("billion", 9),
    ("trillion", 12),
    ("quadrillion", 15),
    ("quintillion", 18),
    ("sextillion", 21),
    ("septillion", 24),
    ("octillion", 27),
    ("nonillion", 30),
    ("decillion", 33),
    ("undecillion", 36),
    ("duodecillion", 39),
    ("tredecillion", 42),
    ("quattuordecillion", 45),
    ("quindecillion", 48),
    ("sexdecillion", 51),
    ("septendecillion", 54),
    ("octodecillion", 57),
    ("novemdecillion", 60),
    ("vigintillion", 63),
    ("centillion", 303),
];

/// `_ORDINAL_TO_CARDINAL`. Stops at "decillionth" even though `_SCALES` runs
/// to "centillion", so "vigintillionth" is an unrecognized token — Python's
/// gap, preserved.
const ORDINAL_TO_CARDINAL: [(&str, &str); 40] = [
    ("zeroth", "zero"),
    ("first", "one"),
    ("second", "two"),
    ("third", "three"),
    ("fourth", "four"),
    ("fifth", "five"),
    ("sixth", "six"),
    ("seventh", "seven"),
    ("eighth", "eight"),
    ("ninth", "nine"),
    ("tenth", "ten"),
    ("eleventh", "eleven"),
    ("twelfth", "twelve"),
    ("thirteenth", "thirteen"),
    ("fourteenth", "fourteen"),
    ("fifteenth", "fifteen"),
    ("sixteenth", "sixteen"),
    ("seventeenth", "seventeen"),
    ("eighteenth", "eighteen"),
    ("nineteenth", "nineteen"),
    ("twentieth", "twenty"),
    ("thirtieth", "thirty"),
    ("fortieth", "forty"),
    ("fiftieth", "fifty"),
    ("sixtieth", "sixty"),
    ("seventieth", "seventy"),
    ("eightieth", "eighty"),
    ("ninetieth", "ninety"),
    ("hundredth", "hundred"),
    ("thousandth", "thousand"),
    ("millionth", "million"),
    ("billionth", "billion"),
    ("trillionth", "trillion"),
    ("quadrillionth", "quadrillion"),
    ("quintillionth", "quintillion"),
    ("sextillionth", "sextillion"),
    ("septillionth", "septillion"),
    ("octillionth", "octillion"),
    ("nonillionth", "nonillion"),
    ("decillionth", "decillion"),
];

/// `_DECIMAL_WORDS`. The base class's `DECIMAL_SEPARATORS` also lists
/// "comma", but `Words2Num_EN._parse` uses this module-level set instead —
/// and `_normalize` turns a literal comma into a space regardless.
const DECIMAL_WORDS: [&str; 2] = ["point", "dot"];
/// `_NEGATIVE_WORDS`.
const NEGATIVE_WORDS: [&str; 2] = ["minus", "negative"];
/// `_AND_WORDS`.
const AND_WORDS: [&str; 1] = ["and"];
/// `_FILLER` — "a hundred" becomes "one hundred".
const FILLER: [&str; 2] = ["a", "an"];

// ---------------------------------------------------------------------------
// Words2Num_EN
// ---------------------------------------------------------------------------

/// Port of `words2num2.lang_EN.Words2Num_EN`.
pub struct W2nLangEn {
    units: HashMap<&'static str, i64>,
    tens: HashMap<&'static str, i64>,
    scales: HashMap<&'static str, BigInt>,
    ordinal_to_cardinal: HashMap<&'static str, &'static str>,
    decimal_words: HashSet<&'static str>,
    negative_words: HashSet<&'static str>,
    and_words: HashSet<&'static str>,
    filler: HashSet<&'static str>,
}

impl Default for W2nLangEn {
    fn default() -> Self {
        Self::new()
    }
}

impl W2nLangEn {
    /// `Words2Num_EN.LANG`.
    pub const LANG: &'static str = "en";
    /// `Words2Num_EN.NEGATIVE_WORDS` (the class attribute; `_parse` reads the
    /// module-level set of the same contents).
    pub const NEGATIVE_WORDS: [&'static str; 2] = NEGATIVE_WORDS;

    pub fn new() -> Self {
        W2nLangEn {
            units: UNITS.iter().copied().collect(),
            tens: TENS.iter().copied().collect(),
            scales: SCALES
                .iter()
                .map(|&(w, p)| (w, BigInt::from(10).pow(p)))
                .collect(),
            ordinal_to_cardinal: ORDINAL_TO_CARDINAL.iter().copied().collect(),
            decimal_words: DECIMAL_WORDS.iter().copied().collect(),
            negative_words: NEGATIVE_WORDS.iter().copied().collect(),
            and_words: AND_WORDS.iter().copied().collect(),
            filler: FILLER.iter().copied().collect(),
        }
    }

    /// `Words2Num_EN.to_cardinal`.
    pub fn to_cardinal(&self, text: &str) -> W2nResult<W2nValue> {
        self.parse(text, false, false)
    }

    /// `Words2Num_EN.to_ordinal`.
    pub fn to_ordinal(&self, text: &str) -> W2nResult<W2nValue> {
        self.parse(text, true, false)
    }

    /// `Words2Num_EN.to_year` — "nineteen ninety nine" is 1999.
    pub fn to_year(&self, text: &str) -> W2nResult<W2nValue> {
        self.parse(text, false, true)
    }

    // ------------------------------------------------------------------
    /// Port of `Words2Num_EN._parse`.
    ///
    /// Python's `_normalize` raises `Words2NumError("expected str, got %r")`
    /// for a non-str argument; a `&str` here makes that unreachable.
    pub fn parse(&self, text: &str, ordinal: bool, year_mode: bool) -> W2nResult<W2nValue> {
        let norm = normalize(text);
        if norm.is_empty() {
            return Err(W2nError::new("empty input"));
        }

        // Pure-digit short circuit. `\d` is Unicode Nd and `int()`/`float()`
        // accept it, so "٤٢" lands here just as "42" does.
        if is_signed_int(&norm) {
            let ascii = nd_to_ascii(&norm);
            return match BigInt::from_str(&ascii) {
                Ok(v) => Ok(W2nValue::Int(v)),
                // Unreachable: the fullmatch above already proved the shape.
                Err(e) => Err(W2nError::new(e.to_string())),
            };
        }
        if is_signed_float(&norm) {
            let ascii = nd_to_ascii(&norm);
            return match f64::from_str(&ascii) {
                // Python's float() yields inf rather than raising when the
                // literal is out of range; Rust's parser agrees.
                Ok(v) => Ok(W2nValue::Float(v)),
                Err(e) => Err(W2nError::new(e.to_string())),
            };
        }

        let mut toks: Vec<&str> = norm.split_whitespace().collect();

        // Negative. Checked before filler removal, so "and minus forty two"
        // keeps its "minus" and later dies in `_cardinal_value`.
        let mut sign = 1i32;
        if let Some(first) = toks.first() {
            if self.negative_words.contains(*first) {
                sign = -1;
                toks.remove(0);
            }
        }
        if toks.is_empty() {
            return Err(W2nError::new("empty input after sign"));
        }

        // Drop pure 'and' / filler in connector positions.
        toks.retain(|t| !self.and_words.contains(*t) && !self.filler.contains(*t));
        if toks.is_empty() {
            return Err(W2nError::new("empty input after filler removal"));
        }

        // Rewrite a trailing ordinal to its cardinal form. This happens
        // whether or not `ordinal` was requested, which is why
        // `to_cardinal("twenty first")` is 21.
        let last = *toks.last().unwrap_or(&"");
        let was_ordinal = self.ordinal_to_cardinal.contains_key(last);
        if was_ordinal {
            let n = toks.len();
            toks[n - 1] = self.ordinal_to_cardinal[last];
        }
        if ordinal && !was_ordinal {
            // Python: a bare `pass`. The caller asked for an ordinal but the
            // form looks cardinal — accepted, because num2words2's ordinal
            // output coincides with the cardinal for 0. Falls through.
        }

        // Decimal split.
        let decimal_idx = toks.iter().position(|t| self.decimal_words.contains(*t));
        if let Some(idx) = decimal_idx {
            let int_toks = &toks[..idx];
            let frac_toks = &toks[idx + 1..];
            let int_part = if int_toks.is_empty() {
                BigInt::from(0)
            } else {
                self.cardinal_value(int_toks)?
            };
            let frac_part = self.fractional_value(frac_toks)?;
            // `sign * (Decimal(int_part) + frac_part)` — always a Decimal,
            // and `-1 * Decimal('0.0')` really is `Decimal('-0.0')`.
            let sum = PyDec::from_bigint(&int_part).add(&frac_part);
            return Ok(W2nValue::Dec(sum.mul_sign(sign < 0)));
        }

        if year_mode && self.looks_like_year(&toks) {
            return Ok(W2nValue::Int(self.year_value(&toks)? * sign));
        }

        Ok(W2nValue::Int(self.cardinal_value(&toks)? * sign))
    }

    // ------------------------------------------------------------------
    /// Port of `Words2Num_EN._cardinal_value`.
    ///
    /// Walks left to right accumulating `current` (the chunk below the next
    /// scale word) and `total`. A scale word folds the chunk into `total`;
    /// "hundred" multiplies the chunk in place.
    fn cardinal_value(&self, toks: &[&str]) -> W2nResult<BigInt> {
        if toks.is_empty() {
            return Err(W2nError::new("empty token list"));
        }
        let mut total = BigInt::from(0);
        let mut current = BigInt::from(0);
        let mut seen_any = false;
        for tok in toks {
            if let Some(&v) = self.units.get(*tok) {
                current += v;
                seen_any = true;
            } else if let Some(&v) = self.tens.get(*tok) {
                current += v;
                seen_any = true;
            } else if *tok == "hundred" {
                if current.sign() == Sign::NoSign {
                    current = BigInt::from(1);
                }
                current *= 100;
                seen_any = true;
            } else if let Some(scale) = self.scales.get(*tok) {
                if current.sign() == Sign::NoSign {
                    current = BigInt::from(1);
                }
                total += &current * scale;
                current = BigInt::from(0);
                seen_any = true;
            } else if is_digits(tok) {
                // Allow embedded digit groups, e.g. "two thousand 24".
                match BigInt::from_str(&nd_to_ascii(tok)) {
                    Ok(v) => current += v,
                    Err(e) => return Err(W2nError::new(e.to_string())),
                }
                seen_any = true;
            } else {
                return Err(W2nError::new(format!(
                    "unrecognized token {} in {}",
                    py_repr(tok),
                    py_repr(&toks.join(" "))
                )));
            }
        }
        if !seen_any {
            // Dead code in Python too: every branch above either sets the
            // flag or returns, and the empty list was rejected up front.
            return Err(W2nError::new("no number tokens in input"));
        }
        Ok(total + current)
    }

    /// Port of `Words2Num_EN._fractional_value` — each token is one digit.
    ///
    /// The `_UNITS[tok] < 10` guard is what rejects "point ten"; "zero",
    /// "oh", "nought" and "naught" all contribute a `0`.
    fn fractional_value(&self, toks: &[&str]) -> W2nResult<PyDec> {
        if toks.is_empty() {
            return Ok(PyDec::zero());
        }
        let mut digits = String::with_capacity(toks.len());
        for tok in toks {
            match self.units.get(*tok) {
                Some(&v) if v < 10 => digits.push_str(&v.to_string()),
                _ if is_single_digit(tok) => digits.push_str(&nd_to_ascii(tok)),
                _ => {
                    return Err(W2nError::new(format!(
                        "unrecognized fractional token {}",
                        py_repr(tok)
                    )))
                }
            }
        }
        // `digits` is non-empty here, so `Decimal("0." + digits)` is valid.
        PyDec::from_frac_digits(&digits)
            .ok_or_else(|| W2nError::new(format!("invalid fractional digits {}", py_repr(&digits))))
    }

    /// Port of `Words2Num_EN._looks_like_year`.
    ///
    /// The docstring claims it checks for "exactly two pair-shaped chunks",
    /// but the code only bounds the token count and rejects the two scale
    /// words. Ported as written.
    fn looks_like_year(&self, toks: &[&str]) -> bool {
        (2..=5).contains(&toks.len())
            && !toks.contains(&"hundred")
            && !toks.contains(&"thousand")
    }

    /// Port of `Words2Num_EN._year_value` — "nineteen ninety nine" is
    /// 19 * 100 + 99.
    ///
    /// Tries every split point and takes the first where the high half is a
    /// 2-digit number and the low half is at most 99. When nothing matches it
    /// falls back to plain cardinal addition, which is why "nine eleven" is
    /// 20 rather than 911.
    fn year_value(&self, toks: &[&str]) -> W2nResult<BigInt> {
        if toks.len() < 2 {
            return self.cardinal_value(toks);
        }
        for split in 1..toks.len() {
            let (high, low) = match (
                self.cardinal_value(&toks[..split]),
                self.cardinal_value(&toks[split..]),
            ) {
                (Ok(h), Ok(l)) => (h, l),
                _ => continue, // Python swallows Words2NumError and moves on
            };
            let in_high = high >= BigInt::from(10) && high <= BigInt::from(99);
            let in_low = low >= BigInt::from(0) && low <= BigInt::from(99);
            if in_high && in_low {
                return Ok(high * 100 + low);
            }
        }
        self.cardinal_value(toks)
    }
}

// ---------------------------------------------------------------------------
// _normalize
// ---------------------------------------------------------------------------

/// `Words2Num_Base._normalize`.
///
/// Already ported in `lib.rs` and reused rather than duplicated:
/// `normalize_py` delegates the NFKD decomposition and combining-mark strip
/// to Python's own `unicodedata` (so the two sides cannot disagree about
/// Unicode), then applies `normalize_tail`. Both are private to the crate
/// root, which makes them visible to this child module.
///
/// The GIL round-trip is free in practice: this crate is a `cdylib` Python
/// extension, so an interpreter is always attached. The fallback exists only
/// so a failure cannot panic under `panic = "abort"` — it skips the NFKD
/// step, which matters solely for accented input ("fórty" -> "forty").
fn normalize(text: &str) -> String {
    pyo3::Python::with_gil(|py| crate::normalize_py(py, text))
        .unwrap_or_else(|_| crate::normalize_tail(text))
}
