"""Shared test fixtures and path constants for the digikam-enricher tests."""

from __future__ import annotations

from pathlib import Path

# tests/ -> parents[0]
# digikam-enricher/ -> parents[1]
# extras/ -> parents[2]
# aplib-extractor (repo root) -> parents[3]
REPO_ROOT = Path(__file__).resolve().parents[3]
TESTDATA = REPO_ROOT / "testdata"
AHTML = TESTDATA / "TestLibrary.aplibrary"