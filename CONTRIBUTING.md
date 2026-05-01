# Contributing to words2num2

words2num2 is the inverse of [num2words2](https://github.com/jqueguiner/num2words2). Contributions are welcome — especially hand-written grammar parsers for individual locales.

## Adding a hand-written parser for a locale

By default every non-English locale uses the generic `Words2Num_Base`,
which derives its lookup table from `num2words2`. This is correct for
small numbers but fails outside the lookup range. To improve a
locale, override the relevant methods in `words2num2/lang_XX.py`:

```python
from .base import Words2Num_Base

class Words2Num_FR(Words2Num_Base):
    LANG = "fr"

    def to_cardinal(self, text):
        # custom French grammar...
        ...
```

## Running tests

```bash
make install-dev
make test
```

Roundtrip tests (in `tests/test_multilang.py`) require `num2words2` to
be installed and verify that `words2num(num2words(n)) == n` for sample
integers in every supported locale.

## Style

Run `make format` (black + isort) before committing.
