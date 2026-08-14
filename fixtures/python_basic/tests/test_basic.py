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


class TestOuter:
    class TestInner:
        def test_nested(self):
            value = 0
            value += 1
            assert value >= 0
