# Local Testing Guide

This document describes how to set up the repository and run the test
suite locally before pushing changes.

## Prerequisites

- Python 3.8 or newer (3.11 recommended; CI matrix is 3.8–3.13).
- `pip` with virtualenv support.

## Setup

```bash
git clone https://github.com/jqueguiner/words2num2
cd words2num2

python3 -m venv .venv
source .venv/bin/activate

# Editable install pulls num2words2 (runtime dep) and dev tooling.
make install-dev
```

`make install-dev` runs:

```bash
pip install -e .
pip install pytest pytest-cov flake8 black isort build twine
```

## Running tests

### Full suite

```bash
make test
# or
pytest tests/
```

### Verbose with per-test names

```bash
make test-verbose
# or
pytest tests/ -v
```

### A single file or a single test

```bash
pytest tests/test_lang_EN.py
pytest tests/test_auto_parse.py::test_currency
pytest tests/test_auto_parse.py -k pluralize
```

### Coverage

```bash
pytest tests/ --cov=words2num2 --cov-report=term-missing
```

This is what the `CI` workflow runs on every push. Aim for the same
local result before tagging a release.

## Test layout

| File | What it covers |
|---|---|
| `tests/test_lang_EN.py` | Hand-written English grammar parser. |
| `tests/test_dispatch.py` | Locale resolution, alias handling, `CONVERTER_CLASSES`. |
| `tests/test_multilang.py` | Roundtrip: `words2num(num2words(n, lang)) == n` across fr/es/de/it/pt/nl/ru/pl. Skipped automatically if `num2words2` is not installed. |
| `tests/test_auto_parse.py` | `auto_parse`, `auto_parse_sentence`, `parse_number_string`, currencies, units, separators, pluralization. |

Total: **135 tests**.

## Linting and formatting

```bash
make lint                  # flake8 (style + safety)
make format                # apply black + isort
```

Pre-commit hooks aren't enforced, but the CI `Lint` workflow runs
`black --check`, `flake8`, `isort --check-only`, and `mypy
--ignore-missing-imports`. Run `make format` before committing to
avoid noise.

## Building the package

```bash
make build
```

Produces:

```
dist/words2num2-X.Y.Z.tar.gz
dist/words2num2-X.Y.Z-py3-none-any.whl
```

`twine check dist/*` runs as part of the build. The CI `Build and
Release` workflow does the same in a clean Ubuntu runner before the
PyPI upload.

## Smoke-testing a built wheel

Before tagging a release, verify the wheel installs and imports
cleanly in an isolated environment:

```bash
python3 -m venv /tmp/w2n-smoke
/tmp/w2n-smoke/bin/pip install dist/words2num2-*.whl

cd /tmp  # leave the repo so cwd doesn't shadow the install
/tmp/w2n-smoke/bin/python -c "
from words2num2 import words2num, auto_parse, __version__
print('version:', __version__)
print(words2num('forty-two'))
print(words2num('quarante-deux', lang='fr'))
print(auto_parse('\$12,345.00'))
"
```

> **Note:** when running `python -c` from inside the repo root, the
> current directory is added to `sys.path` and shadows the installed
> package. Always `cd` out before the smoke test.

## Cutting a release

1. Update `CHANGELOG.md` — move the unreleased entries under a new
   version header.
2. Run the full test suite locally: `make test`.
3. Run the smoke test on a built wheel (above).
4. Tag and push:

   ```bash
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

   The `Build and Release` workflow takes over: build → test → GitHub
   Release → PyPI publish via Trusted Publishing. End-to-end takes
   about 90 seconds.

If Trusted Publishing fails (rare — usually a misconfigured
environment), the `Publish to PyPI (manual)` workflow is a fallback
that uses the `PYPI_API_TOKEN` repo secret.

## Troubleshooting

**`ModuleNotFoundError: No module named 'num2words2'` from the generic backend.**
Install it: `pip install num2words2`. The English path doesn't depend
on it, but every other locale does.

**`Words2NumError: cannot parse '...' as a number`.**
The token isn't in the per-language vocabulary. For non-English locales
the generic backend covers `-1..10000` only — pass an in-range value
or contribute a hand-written parser (see [CONTRIBUTING.md](CONTRIBUTING.md)).

**Smoke test prints the wrong version.**
You're running from inside the repo root. `cd /tmp` (or anywhere
outside the repo) before `python -c` — Python's auto-`sys.path[0]`
shadows the installed package with the source tree.

**Auto-publish workflow on tag push didn't upload to PyPI.**
Check that:
- The PyPI Trusted Publisher has been configured at
  https://pypi.org/manage/project/words2num2/settings/publishing/ with
  workflow `release.yml` and environment `pypi`.
- The GitHub `pypi` environment exists at
  https://github.com/jqueguiner/words2num2/settings/environments.
- The tag matches `v*` exactly (e.g. `v0.2.1`, not `0.2.1`).
