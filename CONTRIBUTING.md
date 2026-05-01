# Contributing to words2num2

`words2num2` is the inverse of [num2words2](https://github.com/jqueguiner/num2words2).
Contributions are very welcome — particularly hand-written grammar
parsers for individual locales, since the default generic backend only
covers a fixed integer range.

## Where to start

| What you want to do | Where to look |
|---|---|
| Add a hand-written parser for a locale | `words2num2/lang_XX.py` (one file per locale) |
| Add a new currency or unit | `words2num2/converters/auto.py` (`UNITS`, `CURRENCIES`) |
| Tweak a per-locale separator default | `words2num2/formats.py` (`NUMBER_FORMAT_DEFAULTS`) |
| Improve sentence-mode regex | `words2num2/converters/sentence.py` (word-form) and `words2num2/converters/auto.py` (digit + unit) |
| Improve pluralization rules | `words2num2/converters/auto.py::pluralize` |

For local setup, testing, and release process, see
[LOCAL_TESTING.md](LOCAL_TESTING.md). For the full API surface, see
[REFERENCE.md](REFERENCE.md).

## Adding a hand-written parser for a locale

By default every non-English locale uses `Words2Num_Base`, which lazily
materializes a `{normalized_words: integer}` lookup table by calling
`num2words2` forward across `LOOKUP_RANGE` (defaults to `-1..10000`).
This is correct inside the window but raises outside it. To improve a
locale, override `to_cardinal` / `to_ordinal` in
`words2num2/lang_XX.py`:

```python
# words2num2/lang_FR.py
from .base import Words2Num_Base, Words2NumError


class Words2Num_FR(Words2Num_Base):
    LANG = "fr"

    def to_cardinal(self, text):
        # Custom French grammar: handles soixante-dix (70),
        # quatre-vingts (80), conjunctions like "et", etc.
        ...
```

Use `lang_EN.py` as the reference implementation. It demonstrates:

- Tokenization + diacritic-normalization via `_normalize`.
- Sign detection (`minus` / `negative`).
- Filler removal (`a`, `an`, `and`).
- Decimal split on `point` / `dot`.
- Year-mode splitting (`nineteen ninety nine` → 1900 + 99).
- Scale-word accumulation (`hundred`, `thousand`, `million`, …).
- Ordinal-to-cardinal mapping (`twenty-first` → 21).

After adding a parser, write tests in `tests/test_lang_XX.py` —
roundtrip examples plus any tricky edge cases the language has.

## Adding a unit or currency

Add a new `UnitInfo` entry to `UNITS`:

```python
"hp": UnitInfo("hp", "horsepower", "power"),
```

Or a `CurrencyInfo` entry to `CURRENCIES`:

```python
"NZD": CurrencyInfo("NZD", "$", "New Zealand dollar"),
```

Update the regex in `auto_parse_sentence` if the new symbol isn't
already covered by `[$€£¥₹₽₩₺]` (currency) or
`[a-zA-Zµ%]+` (unit).

Add tests in `tests/test_auto_parse.py`.

## Number-format defaults

Per-locale defaults live in `NUMBER_FORMAT_DEFAULTS` in
`words2num2/formats.py`. The keys are the locale codes used elsewhere
in the package; values are dicts with `thousands` and `decimal`. Only
add a locale if its conventional thousands/decimal separators differ
from the `_default` (`,` thousands, `.` decimal) — or from the base
language fallback.

## Coding style

- **Black + isort** with the project's `pyproject.toml` config —
  `make format` applies both.
- **Flake8** with the project's `.flake8` — `make lint` checks.
- Type hints are encouraged but not enforced by CI; mypy runs with
  `--ignore-missing-imports`.

Avoid adding new top-level dependencies; the runtime list is
intentionally short (`docopt`, `num2words2`).

## Running the test suite

```bash
make install-dev
make test
```

Detail in [LOCAL_TESTING.md](LOCAL_TESTING.md).

## Release process

Maintainers cut releases by tagging:

```bash
git tag vX.Y.Z
git push origin vX.Y.Z
```

This fires the `Build and Release` GitHub Actions workflow, which
builds, tests, creates a GitHub Release, and uploads to PyPI via
Trusted Publishing. Pre-flight checklist:

- [ ] All tests pass (`make test`).
- [ ] `CHANGELOG.md` updated.
- [ ] README / REFERENCE updated if public API changed.
- [ ] Smoke test on a built wheel from outside the repo.
