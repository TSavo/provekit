<?php

declare(strict_types=1);

$vendorAutoload = dirname(__DIR__, 2) . '/vendor/autoload.php';
if (is_file($vendorAutoload)) {
    require_once $vendorAutoload;
}

require_once dirname(__DIR__, 2) . '/provekit-ir-symbolic/src/Canonicalizer/Jcs.php';
require_once dirname(__DIR__, 2) . '/provekit-ir-symbolic/src/Canonicalizer/Blake3.php';

require_once __DIR__ . '/Ir.php';
require_once __DIR__ . '/EffectSet.php';
require_once __DIR__ . '/PhpSourceCompiler.php';
require_once __DIR__ . '/PhpSourceLifter.php';
require_once __DIR__ . '/PhpSourceRecognizer.php';
require_once __DIR__ . '/Rpc.php';
