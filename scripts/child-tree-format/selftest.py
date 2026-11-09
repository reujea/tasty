#!/usr/bin/env python3
"""Run every self test of the child tree format scripts.

Exit codes: 0 all passed, 1 a test failed, 2 no test was found. A run with no
tests must not read as a pass.
"""

import sys
import unittest
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent


def main():
    sys.path.insert(0, str(HERE))
    suite = unittest.defaultTestLoader.discover(str(HERE / "tests"), pattern="test_*.py")
    count = suite.countTestCases()
    if count == 0:
        print("cannot judge: no tests found", file=sys.stderr)
        return 2
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    failed = len(result.failures) + len(result.errors)
    print(f"selftest: tests={result.testsRun} failed={failed} skipped={len(result.skipped)}")
    return 0 if result.wasSuccessful() and result.testsRun > 0 else 1


if __name__ == "__main__":
    sys.exit(main())
