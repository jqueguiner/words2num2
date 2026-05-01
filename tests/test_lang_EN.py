# -*- coding: utf-8 -*-
"""English words-to-number tests."""
from decimal import Decimal

import pytest

from words2num2 import words2num, words2num_sentence
from words2num2.base import Words2NumError


@pytest.mark.parametrize(
    "text,expected",
    [
        ("zero", 0),
        ("one", 1),
        ("nine", 9),
        ("ten", 10),
        ("nineteen", 19),
        ("twenty", 20),
        ("forty-two", 42),
        ("ninety-nine", 99),
        ("one hundred", 100),
        ("one hundred and one", 101),
        ("two hundred fifty", 250),
        ("a hundred", 100),
        ("one thousand", 1000),
        ("one thousand two hundred thirty-four", 1234),
        ("ten thousand", 10_000),
        ("one hundred thousand", 100_000),
        ("one million", 1_000_000),
        ("two million three hundred thousand", 2_300_000),
        ("one billion", 1_000_000_000),
    ],
)
def test_cardinal(text, expected):
    assert words2num(text) == expected


@pytest.mark.parametrize(
    "text,expected",
    [
        ("minus one", -1),
        ("negative seven", -7),
        ("minus one hundred", -100),
    ],
)
def test_negative(text, expected):
    assert words2num(text) == expected


@pytest.mark.parametrize(
    "text,expected",
    [
        ("first", 1),
        ("second", 2),
        ("third", 3),
        ("twenty-first", 21),
        ("one hundredth", 100),
        ("one millionth", 1_000_000),
    ],
)
def test_ordinal(text, expected):
    assert words2num(text, to="ordinal") == expected
    assert words2num(text) == expected  # cardinal also accepts ordinal forms


def test_decimal():
    assert words2num("three point one four") == Decimal("3.14")
    assert words2num("zero point five") == Decimal("0.5")


def test_year():
    assert words2num("nineteen ninety nine", to="year") == 1999
    assert words2num("two thousand twenty four", to="year") == 2024


def test_digits_passthrough():
    assert words2num("42") == 42
    assert words2num("-17") == -17
    assert words2num("3.14") == 3.14


def test_unknown_token_raises():
    with pytest.raises(Words2NumError):
        words2num("forty zoot")


def test_empty_input_raises():
    with pytest.raises(Words2NumError):
        words2num("")


def test_sentence_simple():
    assert (
        words2num_sentence("I bought twenty-three apples.")
        == "I bought 23 apples."
    )


def test_sentence_multiple_runs():
    assert (
        words2num_sentence("I bought twenty-three apples and fourteen pears.")
        == "I bought 23 apples and 14 pears."
    )


def test_sentence_preserves_punctuation():
    assert (
        words2num_sentence(
            "In nineteen ninety nine, two thousand people came.", to="year"
        )
        == "In 1999, 2000 people came."
    )
