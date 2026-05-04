<?php
/** RFC 8785 JCS-JSON canonicalizer. Mirrors the Go canonicalizer. */

namespace ProvekIt\Canonicalizer;

class Jcs
{
    const MAX_DEPTH = 64;

    /** Serialize a value to canonical JCS-JSON per RFC 8785. */
    public static function encode(mixed $v, int $depth = 0): string
    {
        if ($depth > self::MAX_DEPTH) throw new \RuntimeException('max depth exceeded');

        if (is_null($v)) return 'null';
        if (is_bool($v)) return $v ? 'true' : 'false';
        if (is_int($v) || is_float($v)) {
            // RFC 8785: JSON numbers MUST NOT have leading zeros, fractions,
            // or exponents by default. We keep integers as-is.
            // PHP json_encode handles this correctly for ints.
            return (string)(json_encode($v, JSON_PRESERVE_ZERO_FRACTION));
        }
        if (is_string($v)) return json_encode($v, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
        if ($v instanceof \stdClass) $v = (array)$v;

        if (is_array($v)) {
            if (array_is_list($v)) {
                $parts = [];
                foreach ($v as $item) $parts[] = self::encode($item, $depth + 1);
                return '[' . implode(',', $parts) . ']';
            }

            // Object: sort keys per RFC 8785
            $keys = array_keys($v);
            sort($keys, SORT_STRING);
            $pairs = [];
            foreach ($keys as $k) {
                $pairs[] = json_encode((string)$k) . ':' . self::encode($v[$k], $depth + 1);
            }
            return '{' . implode(',', $pairs) . '}';
        }

        if (is_object($v)) {
            if ($v instanceof \JsonSerializable) {
                $arr = $v->jsonSerialize();
                return self::encode($arr, $depth);
            }
            $arr = (array)$v;
            return self::encode($arr, $depth);
        }

        throw new \RuntimeException('unhandled type: ' . gettype($v));
    }
}
