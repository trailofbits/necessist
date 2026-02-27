<?php

class BasicTest
{
    public function testPassed()
    {
        $n = 0;
        $n += 1;
        noop();
    }

    public function testFailed()
    {
        $n = 0;
        $n += 1;
        assert($n >= 1);
    }
}

function noop() {}
