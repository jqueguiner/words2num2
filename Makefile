.PHONY: help install install-dev test test-verbose lint format clean build publish-test publish

help:
	@echo "words2num2 - inverse of num2words2"
	@echo ""
	@echo "Targets:"
	@echo "  install       Install the package"
	@echo "  install-dev   Install with dev dependencies"
	@echo "  test          Run the test suite"
	@echo "  test-verbose  Run tests with -v"
	@echo "  lint          Run flake8"
	@echo "  format        Run black + isort"
	@echo "  clean         Remove build artifacts"
	@echo "  build         Build sdist + wheel"
	@echo "  publish-test  Upload to TestPyPI"
	@echo "  publish       Upload to PyPI"

install:
	pip install -e .

install-dev:
	pip install -e ".[dev]" || pip install -e .
	pip install pytest pytest-cov flake8 black isort build twine

test:
	pytest tests/

test-verbose:
	pytest tests/ -v

lint:
	flake8 words2num2 tests

format:
	isort words2num2 tests
	black words2num2 tests

clean:
	rm -rf build dist *.egg-info .pytest_cache .coverage htmlcov
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete

build: clean
	python -m build

publish-test: build
	twine upload --repository testpypi dist/*

publish: build
	twine upload dist/*
