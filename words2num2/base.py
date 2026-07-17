# -*- coding: utf-8 -*-
# Copyright (c) 2026, Jean-Louis Queguiner. All Rights Reserved.
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
"""The one exception type the binder surfaces to Python.

``words2num2`` is a thin binder over the compiled ``_rust`` core; every parser
lives there. The core raises this error by importing the class from Python (see
``rust/words2num2-py/src/lib.rs``, ``words2num_error``), so it must stay
importable as ``words2num2.base.Words2NumError``.
"""
from __future__ import unicode_literals


class Words2NumError(ValueError):
    """Raised when an input string cannot be parsed as a number."""
