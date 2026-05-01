# -*- coding: utf-8 -*-
"""Tests for auto_parse, auto_parse_sentence, parse_number_string."""
import pytest

from words2num2 import (
    Quantity,
    auto_parse,
    auto_parse_sentence,
    parse_number_string,
)
from words2num2.base import Words2NumError


# ---------------------------------------------------------------------------
# Currency
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "text,value,unit,kwargs",
    [
        ("$12.50", 12.5, "USD", {}),
        ("$12,345.00", 12345.0, "USD", {}),
        ("€42", 42, "EUR", {}),
        ("€12,50", 12.5, "EUR", {"lang": "de"}),  # comma decimal in DE locale
        ("£1,000.99", 1000.99, "GBP", {}),
        ("¥500", 500, "JPY", {}),
        ("USD 100", 100, "USD", {}),
        ("100 USD", 100, "USD", {}),
        ("12.50 €", 12.5, "EUR", {}),
    ],
)
def test_currency(text, value, unit, kwargs):
    q = auto_parse(text, **kwargs)
    assert q.value == value
    assert q.unit == unit
    assert q.kind == "currency"


@pytest.mark.parametrize(
    "text,value",
    [
        ("$5k", 5_000),
        ("$5m", 5_000_000),
        ("$5b", 5_000_000_000),
        ("$5bn", 5_000_000_000),
        ("$1.5b", 1.5e9),
        ("$2.5t", 2.5e12),
    ],
)
def test_currency_scale_shortcuts(text, value):
    q = auto_parse(text)
    assert q.value == value
    assert q.kind == "currency"


# ---------------------------------------------------------------------------
# Units
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "text,value,unit,kind",
    [
        ("5cm", 5, "cm", "length"),
        ("5 cm", 5, "cm", "length"),
        ("3.5 km", 3.5, "km", "length"),
        ("100mm", 100, "mm", "length"),
        ("42%", 42, "%", "percent"),
        ("70 %", 70, "%", "percent"),
        ("20°C", 20, "°C", "temperature"),
        ("-40°F", -40, "°F", "temperature"),
        ("5kg", 5, "kg", "mass"),
        ("250ml", 250, "ml", "volume"),
        ("3L", 3, "L", "volume"),
        ("30min", 30, "min", "time"),
        ("100ms", 100, "ms", "time"),
    ],
)
def test_unit_digit_form(text, value, unit, kind):
    q = auto_parse(text)
    assert q.value == value
    assert q.unit == unit
    assert q.kind == kind


@pytest.mark.parametrize(
    "text,value,unit",
    [
        ("forty-two kg", 42, "kg"),
        ("twenty-three cm", 23, "cm"),
        ("five centimeters", 5, "cm"),
        ("ten kilometers", 10, "km"),
        ("twelve dollars", 12, "USD"),
        ("twenty-three percent", 23, "%"),
    ],
)
def test_unit_word_form(text, value, unit):
    q = auto_parse(text)
    assert q.value == value
    assert q.unit == unit


def test_unit_long_expansion():
    q = auto_parse("5cm")
    assert q.unit_long == "centimeter"
    q = auto_parse("$12.50")
    assert q.unit_long == "dollar"


def test_prefer_overrides_ambiguous_unit():
    q = auto_parse("5m", prefer={"m": "mile"})
    assert q.value == 5
    assert q.unit_long == "mile"


# ---------------------------------------------------------------------------
# Number-string parsing with separators
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "text,kwargs,expected",
    [
        ("12,345.67", {}, 12345.67),
        ("12.345,67", {"lang": "de"}, 12345.67),
        ("1 234,56", {"lang": "fr"}, 1234.56),
        ("12'345.67", {"thousands_sep": "'", "decimal_sep": "."}, 12345.67),
        ("1_234.56", {"thousands_sep": "_"}, 1234.56),
        ("1,234,567.89", {}, 1234567.89),
        ("1.234.567,89", {"lang": "de"}, 1234567.89),
        ("-12,345.67", {}, -12345.67),
        ("+1.5", {}, 1.5),
        ("0.5", {}, 0.5),
    ],
)
def test_parse_number_string(text, kwargs, expected):
    assert parse_number_string(text, **kwargs) == expected


def test_parse_number_string_invalid():
    with pytest.raises(Words2NumError):
        parse_number_string("")
    with pytest.raises(Words2NumError):
        parse_number_string("not a number")


# ---------------------------------------------------------------------------
# Sentence mode
# ---------------------------------------------------------------------------


def test_sentence_currency_and_unit():
    out = auto_parse_sentence(
        "The package weighs 5kg and costs $12.50."
    )
    assert "5 kg" in out
    assert "12.5 USD" in out


def test_sentence_preserves_punctuation():
    out = auto_parse_sentence("Cost: $12.50, weight: 30kg.")
    assert "12.5 USD" in out
    assert "30 kg" in out
    assert ", " in out  # comma+space preserved between
    assert out.endswith(".")


def test_sentence_percent_no_space():
    out = auto_parse_sentence("Humidity 70%.")
    assert "70%" in out


@pytest.mark.parametrize(
    "value,singular,expected",
    [
        (1, "dollar", "dollar"),
        (0, "dollar", "dollars"),
        (5, "dollar", "dollars"),
        (1.5, "dollar", "dollars"),
        (-1, "dollar", "dollar"),
        (-5, "dollar", "dollars"),
        (5, "foot", "feet"),
        (1, "foot", "foot"),
        (5, "inch", "inches"),
        (1, "inch", "inch"),
        (5, "kilogram", "kilograms"),
        (5, "yen", "yen"),
        (1, "yen", "yen"),
        (5, "yuan", "yuan"),
        (5, "won", "won"),
        (5, "kelvin", "kelvin"),
        (5, "percent", "percent"),
        (5, "degree celsius", "degrees celsius"),
        (5, "pound sterling", "pounds sterling"),
        (5, "Swiss franc", "Swiss francs"),
    ],
)
def test_pluralize(value, singular, expected):
    from words2num2.converters.auto import pluralize
    assert pluralize(singular, value) == expected


def test_sentence_expand_pluralizes():
    out = auto_parse_sentence("Pay $12.50 for 5kg.", expand=True)
    assert "dollars" in out
    assert "kilograms" in out

    out = auto_parse_sentence("Pay $1.00 for 1kg.", expand=True)
    assert "dollar" in out and "dollars" not in out
    assert "kilogram" in out and "kilograms" not in out

    out = auto_parse_sentence("5 ft and 1 ft.", expand=True)
    assert "5 feet" in out
    assert "1 foot" in out

    out = auto_parse_sentence("Pay 100 yen.", expand=True)
    assert "100 yen" in out


def test_sentence_expand_mode():
    out = auto_parse_sentence("Pay $12.50 for 5kg.", expand=True)
    assert "dollar" in out
    assert "kilogram" in out


def test_sentence_locale_separators_de():
    out = auto_parse_sentence("Total: 1.234,56 €.", lang="de")
    assert "1234.56" in out
    assert "EUR" in out


def test_sentence_locale_separators_fr():
    out = auto_parse_sentence("Total : 1 234,56 €.", lang="fr")
    assert "1234.56" in out
    assert "EUR" in out


# ---------------------------------------------------------------------------
# Quantity dataclass
# ---------------------------------------------------------------------------


def test_quantity_repr_with_unit():
    q = Quantity(value=5, unit="kg", kind="mass")
    assert "kg" in repr(q)
    assert "mass" in repr(q)


def test_quantity_repr_plain():
    q = Quantity(value=42)
    assert "42" in repr(q)
