# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""Public result type + pluralize helper for the auto-parse API.

The whole extraction pipeline (currency forms, scale shortcuts, digit/word +
unit suffixes, per-locale separators, ``prefer`` disambiguation) is a full port
in the Rust core (``w2n_sentence.rs``). This module keeps only the public
:class:`Quantity` result type — which the core constructs by name (see
``rust/words2num2-py/src/lib.rs``, ``quantity_to_py``), so it stays the single
source of truth for the result shape — and the thin :func:`pluralize` binder.
"""
from __future__ import unicode_literals

from dataclasses import dataclass
from decimal import Decimal
from typing import Optional, Union


@dataclass
class Quantity:
    value: Union[int, float, Decimal]
    unit: Optional[str] = None
    unit_long: Optional[str] = None
    kind: Optional[str] = None
    confidence: float = 1.0
    raw: str = ""

    def __repr__(self):
        if self.unit:
            return "Quantity(value={!r}, unit={!r}, kind={!r}, confidence={})".format(
                self.value, self.unit, self.kind, self.confidence
            )
        return "Quantity(value={!r})".format(self.value)


def pluralize(long_form, value):
    """Return ``long_form`` pluralized for ``value`` (English rules: singular
    at +/-1, irregulars like foot->feet, uncountables like yen). Thin binder
    over the core so the standalone helper and ``auto_parse_sentence`` share one
    implementation."""
    from .. import _rust as _RUST

    return _RUST.pluralize(long_form, value)
