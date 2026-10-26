"""`python -m bullterrier_control`: the same command line as the `bt-control` program."""

import sys

from .cli import main

sys.exit(main())
