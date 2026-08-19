# This file verifies that a Python `skip-file` directive is honored. Without the directive, the
# candidates are those of `fixtures/python_basic`.
# necessist: skip-file


def add_one(value):
    value += 1
    return value


def test_passed():
    value = 0
    value += 1
    add_one(value)
    str(value).strip()
    assert value >= 0


class TestBasic:
    def test_failed(self):
        value = 0
        value += 1
        assert value >= 1
