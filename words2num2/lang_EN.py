# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""English words-to-number parser.

Handles cardinals, ordinals, decimals, negatives, and scale words up to
'centillion'. Designed to mirror num2words2.lang_EN as the reference
locale.
"""
from __future__ import unicode_literals

import re
from decimal import Decimal

from .base import Words2Num_Base, Words2NumError

_UNITS = {
    "zero": 0, "oh": 0, "nought": 0, "naught": 0,
    "one": 1, "two": 2, "three": 3, "four": 4, "five": 5,
    "six": 6, "seven": 7, "eight": 8, "nine": 9,
    "ten": 10, "eleven": 11, "twelve": 12, "thirteen": 13,
    "fourteen": 14, "fifteen": 15, "sixteen": 16, "seventeen": 17,
    "eighteen": 18, "nineteen": 19,
}

_TENS = {
    "twenty": 20, "thirty": 30, "forty": 40, "fifty": 50,
    "sixty": 60, "seventy": 70, "eighty": 80, "ninety": 90,
}

_SCALES = {
    "hundred": 100,
    "thousand": 10 ** 3,
    "million": 10 ** 6,
    "billion": 10 ** 9,
    "trillion": 10 ** 12,
    "quadrillion": 10 ** 15,
    "quintillion": 10 ** 18,
    "sextillion": 10 ** 21,
    "septillion": 10 ** 24,
    "octillion": 10 ** 27,
    "nonillion": 10 ** 30,
    "decillion": 10 ** 33,
    "undecillion": 10 ** 36,
    "duodecillion": 10 ** 39,
    "tredecillion": 10 ** 42,
    "quattuordecillion": 10 ** 45,
    "quindecillion": 10 ** 48,
    "sexdecillion": 10 ** 51,
    "septendecillion": 10 ** 54,
    "octodecillion": 10 ** 57,
    "novemdecillion": 10 ** 60,
    "vigintillion": 10 ** 63,
    "centillion": 10 ** 303,
}

# Ordinal → cardinal word (for re-using the cardinal grammar after rewriting
# the trailing ordinal into its base form).
_ORDINAL_TO_CARDINAL = {
    "zeroth": "zero",
    "first": "one", "second": "two", "third": "three",
    "fourth": "four", "fifth": "five", "sixth": "six",
    "seventh": "seven", "eighth": "eight", "ninth": "nine",
    "tenth": "ten", "eleventh": "eleven", "twelfth": "twelve",
    "thirteenth": "thirteen", "fourteenth": "fourteen",
    "fifteenth": "fifteen", "sixteenth": "sixteen",
    "seventeenth": "seventeen", "eighteenth": "eighteen",
    "nineteenth": "nineteen",
    "twentieth": "twenty", "thirtieth": "thirty", "fortieth": "forty",
    "fiftieth": "fifty", "sixtieth": "sixty", "seventieth": "seventy",
    "eightieth": "eighty", "ninetieth": "ninety",
    "hundredth": "hundred", "thousandth": "thousand",
    "millionth": "million", "billionth": "billion",
    "trillionth": "trillion", "quadrillionth": "quadrillion",
    "quintillionth": "quintillion", "sextillionth": "sextillion",
    "septillionth": "septillion", "octillionth": "octillion",
    "nonillionth": "nonillion", "decillionth": "decillion",
}

_DECIMAL_WORDS = {"point", "dot"}
_NEGATIVE_WORDS = {"minus", "negative"}
_AND_WORDS = {"and"}
_FILLER = {"a", "an"}  # "a hundred" → "one hundred"


class Words2Num_EN(Words2Num_Base):
    LANG = "en"
    NEGATIVE_WORDS = ("minus", "negative")

    def to_cardinal(self, text):
        return self._parse(text, ordinal=False)

    def to_ordinal(self, text):
        return self._parse(text, ordinal=True)

    def to_year(self, text):
        # Years like "nineteen ninety nine" → 1999. Try cardinal first;
        # if the cardinal parser yields a value but the input is two
        # natural pairs (e.g. "nineteen ninety nine"), accept it.
        return self._parse(text, ordinal=False, year_mode=True)

    # ------------------------------------------------------------------
    def _parse(self, text, ordinal=False, year_mode=False):
        norm = self._normalize(text)
        if not norm:
            raise Words2NumError("empty input")

        # Pure-digit short circuit.
        if re.fullmatch(r"-?\d+", norm):
            return int(norm)
        if re.fullmatch(r"-?\d+\.\d+", norm):
            return float(norm)

        toks = norm.split()

        # Negative.
        sign = 1
        if toks and toks[0] in _NEGATIVE_WORDS:
            sign = -1
            toks = toks[1:]
        if not toks:
            raise Words2NumError("empty input after sign")

        # Drop pure 'and' / filler in connector positions.
        toks = [t for t in toks if t not in _AND_WORDS and t not in _FILLER]
        if not toks:
            raise Words2NumError("empty input after filler removal")

        # Replace ordinal-typed last token with its cardinal form when
        # the caller asked for ordinal parsing OR the trailing token is
        # ordinal-shaped (e.g. "twenty first").
        last = toks[-1]
        was_ordinal = last in _ORDINAL_TO_CARDINAL
        if was_ordinal:
            toks = toks[:-1] + [_ORDINAL_TO_CARDINAL[last]]
        if ordinal and not was_ordinal:
            # Caller requested ordinal but the form looks cardinal — accept,
            # since num2words2 ordinal output sometimes coincides with cardinal
            # for 0 ("zeroth" vs. "zero"); fall through.
            pass

        # Decimal split.
        decimal_idx = next(
            (i for i, t in enumerate(toks) if t in _DECIMAL_WORDS), None
        )
        if decimal_idx is not None:
            int_toks = toks[:decimal_idx]
            frac_toks = toks[decimal_idx + 1:]
            int_part = self._cardinal_value(int_toks) if int_toks else 0
            frac_part = self._fractional_value(frac_toks)
            return sign * (Decimal(int_part) + frac_part)

        if year_mode and self._looks_like_year(toks):
            return sign * self._year_value(toks)

        return sign * self._cardinal_value(toks)

    # ------------------------------------------------------------------
    def _cardinal_value(self, toks):
        """Parse a sequence of cardinal English tokens into an integer.

        Algorithm: walk left to right, accumulating ``current`` (a "chunk"
        below the next scale word) and ``total``. When a scale token is
        seen, multiply the current chunk by the scale and either fold it
        into ``total`` (for thousand+) or keep building (for hundred).
        """
        if not toks:
            raise Words2NumError("empty token list")
        total = 0
        current = 0
        seen_any = False
        for tok in toks:
            if tok in _UNITS:
                current += _UNITS[tok]
                seen_any = True
            elif tok in _TENS:
                current += _TENS[tok]
                seen_any = True
            elif tok == "hundred":
                if current == 0:
                    current = 1
                current *= 100
                seen_any = True
            elif tok in _SCALES:
                scale = _SCALES[tok]
                if current == 0:
                    current = 1
                total += current * scale
                current = 0
                seen_any = True
            else:
                # Allow embedded digit groups, e.g. "two thousand 24".
                if re.fullmatch(r"\d+", tok):
                    current += int(tok)
                    seen_any = True
                    continue
                raise Words2NumError(
                    "unrecognized token %r in %r" % (tok, " ".join(toks))
                )
        if not seen_any:
            raise Words2NumError("no number tokens in input")
        return total + current

    def _fractional_value(self, toks):
        """Parse decimal-side tokens. Each token represents a single digit."""
        if not toks:
            return Decimal(0)
        digits = []
        for tok in toks:
            if tok in _UNITS and _UNITS[tok] < 10:
                digits.append(str(_UNITS[tok]))
            elif re.fullmatch(r"\d", tok):
                digits.append(tok)
            else:
                raise Words2NumError(
                    "unrecognized fractional token %r" % tok
                )
        return Decimal("0." + "".join(digits))

    def _looks_like_year(self, toks):
        # Heuristic: exactly two pair-shaped chunks like "nineteen ninety nine".
        return 2 <= len(toks) <= 5 and "hundred" not in toks and (
            "thousand" not in toks
        )

    def _year_value(self, toks):
        # "nineteen ninety nine" -> 19 * 100 + 99
        # Split into halves: first half before a tens-or-teen pivot.
        # Simple approach: parse the first group of consecutive unit/teen/tens
        # as 'high' and the rest as 'low'.
        if len(toks) < 2:
            return self._cardinal_value(toks)
        # Try every split point; pick the one where both halves parse and
        # high is in [10, 99].
        for split in range(1, len(toks)):
            try:
                high = self._cardinal_value(toks[:split])
                low = self._cardinal_value(toks[split:])
            except Words2NumError:
                continue
            if 10 <= high <= 99 and 0 <= low <= 99:
                return high * 100 + low
        return self._cardinal_value(toks)
