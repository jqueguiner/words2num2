# -*- coding: utf-8 -*-
"""Tests for the locale dispatch (now entirely in the Rust core)."""
import pytest

from words2num2 import words2num


def test_unknown_lang_raises():
    with pytest.raises(NotImplementedError):
        words2num("forty-two", lang="xx")


def test_dash_normalization():
    assert words2num("forty-two", lang="en") == 42
    # Hyphenated locale forms like en-US fall back to "en".
    assert words2num("forty-two", lang="en-US") == 42
