"""pytest wrapper: every built-in self test becomes one pytest case."""

import pytest

from oopsmath_stage.selftest import TESTS


@pytest.mark.parametrize("name,test", TESTS, ids=[n for n, _ in TESTS])
def test_builtin(name, test):
    test()
