<?php

use PHPUnit\Framework\TestCase;

class BasicTest extends TestCase
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
        $this->assertTrue($n >= 1);
    }
}

function noop() {}
