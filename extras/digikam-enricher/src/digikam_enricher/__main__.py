"""CLI entry point for the ``digikam-enricher`` console script."""

import sys

from .cli import main

if __name__ == "__main__":
    raise SystemExit(main())