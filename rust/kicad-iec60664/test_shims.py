#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Run the shared complete-reply input contract against both live adapters."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "port/oracle"))
from test_iec60664_inputs import main

if __name__ == "__main__":
    main()
