"""What the format tests share."""

import re

import pytest


@pytest.fixture
def reports():
    """`reports(error, "series.bass.rate")` fails unless the error lists that field path (`path: rule`, `path = value`
    or an index into it), so a name that merely appears in the echoed input does not count."""

    def check(error: Exception, field: str) -> None:
        pattern = rf"(?<![\w'\"]){re.escape(field)}(?:: | = |\[|\.)"
        assert re.search(pattern, str(error)), f"{field!r} is not reported as a path in: {error}"

    return check
