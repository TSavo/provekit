<?php

declare(strict_types=1);

require_once __DIR__ . '/../src/bootstrap.php';

use function ProvekIt\LiftPhpSource\run_rpc;

if (!in_array('--rpc', $argv, true)) {
    fwrite(STDERR, "usage: php provekit-lift-php-source/bin/main.php --rpc\n");
    exit(1);
}

run_rpc();
