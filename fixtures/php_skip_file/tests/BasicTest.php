<?php

// This file verifies that a `skip-file` directive is honored when preceded by `<?php`. Without the
// directive, the candidates are those of `fixtures/php_basic`.
// necessist: skip-file

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
