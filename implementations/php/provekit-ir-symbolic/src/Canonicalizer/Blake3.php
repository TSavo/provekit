<?php
/** ProvekIt — BLAKE3-512 hasher. Uses b3sum/blake3 CLI until a native binding lands. */

namespace ProvekIt\Canonicalizer;

class Blake3
{
    /** BLAKE3-512 hash of bytes, returned as hex string. */
    public static function hash(string $data): string
    {
        foreach ([['b3sum', '--length', '64', '--no-names'], ['blake3', '--length', '64', '--no-names']] as $cmd) {
            $exe = self::resolveBinary($cmd[0]);
            if ($exe === null) {
                continue;
            }
            $cmd[0] = $exe;
            $proc = proc_open(
                $cmd,
                [0 => ['pipe', 'r'], 1 => ['pipe', 'w'], 2 => ['pipe', 'w']],
                $pipes
            );
            if (!is_resource($proc)) {
                continue;
            }
            fwrite($pipes[0], $data);
            fclose($pipes[0]);
            $out = stream_get_contents($pipes[1]);
            fclose($pipes[1]);
            fclose($pipes[2]);
            $code = proc_close($proc);
            $hex = trim($out);
            if ($code === 0 && preg_match('/^[0-9a-f]{128}$/', $hex)) {
                return $hex;
            }
        }

        throw new \RuntimeException('BLAKE3-512 implementation not available; install b3sum or blake3 CLI');
    }

    private static function resolveBinary(string $name): ?string
    {
        $dirs = array_filter(explode(PATH_SEPARATOR, getenv('PATH') ?: ''));
        $home = getenv('HOME');
        if ($home !== false && $home !== '') {
            $dirs[] = $home . '/.cargo/bin';
        }
        array_push($dirs, '/usr/local/bin', '/opt/homebrew/bin', '/usr/bin', '/bin');
        foreach (array_unique($dirs) as $dir) {
            $candidate = rtrim($dir, DIRECTORY_SEPARATOR) . DIRECTORY_SEPARATOR . $name;
            if (is_executable($candidate)) {
                return $candidate;
            }
        }
        return null;
    }

    /** Full CID string: "blake3-512:<hex>" */
    public static function cid(string $data): string
    {
        return 'blake3-512:' . self::hash($data);
    }

    public static function hashKey(string $data): string
    {
        return pack('H*', self::hash($data));
    }
}
