# -*- coding: utf-8 -*-
"""Tests for the locale dispatch."""
import pytest

from words2num2 import CONVERTER_CLASSES, words2num


def test_locale_count():
    # 100+ locales registered.
    assert len(CONVERTER_CLASSES) >= 100


def test_aliases():
    assert "jp" in CONVERTER_CLASSES
    assert "cn" in CONVERTER_CLASSES
    assert type(CONVERTER_CLASSES["jp"]) is type(CONVERTER_CLASSES["ja"])
    assert type(CONVERTER_CLASSES["cn"]) is type(CONVERTER_CLASSES["zh_CN"])


def test_unknown_lang_raises():
    with pytest.raises(NotImplementedError):
        words2num("forty-two", lang="xx")


def test_dash_normalization():
    assert words2num("forty-two", lang="en") == 42
    # Hyphenated locale forms like en-US fall back to "en".
    assert words2num("forty-two", lang="en-US") == 42
