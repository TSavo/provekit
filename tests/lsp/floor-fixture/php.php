<?php
// Forward-propagation floor fixture for PHP
// Tests: (1) callsite satisfies pre, no diagnostic | (2) callsite violates pre, diagnostic | (3) loop path, top fallback

function checkPositive(int $x): bool {
    if ($x <= 0) { return false; }  // pre: x > 0
    return true;
}

function callerSatisfiesPre(): bool {
    $result = checkPositive(5);  // satisfies pre (x=5 > 0)
    return $result;
}

function callerViolatesPre(): bool {
    $result = checkPositive(-1);  // violates pre (x=-1 <= 0)
    return $result;
}

function callerWithLoop(): bool {
    for ($i = 0; $i < 10; $i++) {
        $result = checkPositive($i);  // top fallback at loop entry
        if (!$result) { return false; }
    }
    return true;
}