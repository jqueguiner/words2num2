# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
"""ES words-to-number parser.

Default implementation: generic reverse-lookup driven by num2words2.
For grammar-precise parsing (out-of-range values, ordinals, conjunctions),
override :meth:`to_cardinal` and friends.
"""
from __future__ import unicode_literals

from .base import Words2Num_Base


class Words2Num_ES(Words2Num_Base):
    LANG = "es"
