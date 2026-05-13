<?php

declare(strict_types=1);

namespace ProvekIt\LiftPhpSource;

use ProvekIt\Canonicalizer\Jcs;

final class EffectSet
{
    /** @var array<string, array> */
    private array $effects = [];

    public function addRead(string $target): void
    {
        $this->add(['kind' => 'reads', 'target' => $target]);
    }

    public function addWrite(string $target): void
    {
        $this->add(['kind' => 'writes', 'target' => $target]);
    }

    public function addIo(): void
    {
        $this->add(['kind' => 'io']);
    }

    public function addPanics(): void
    {
        $this->add(['kind' => 'panics']);
    }

    public function addUnresolvedCall(string $name): void
    {
        $this->add(['kind' => 'unresolved_call', 'name' => $name]);
    }

    public function addOpaqueLoop(string $loopCid): void
    {
        $this->add(['kind' => 'opaque_loop', 'loopCid' => $loopCid]);
    }

    /**
     * @return array<int, array>
     */
    public function all(): array
    {
        $effects = array_values($this->effects);
        usort($effects, static fn(array $a, array $b): int => self::sortKey($a) <=> self::sortKey($b));
        return $effects;
    }

    private function add(array $effect): void
    {
        $this->effects[Jcs::encode($effect)] = $effect;
    }

    private static function sortKey(array $effect): string
    {
        return match ($effect['kind'] ?? '') {
            'reads' => '0:reads:' . ($effect['target'] ?? ''),
            'writes' => '1:writes:' . ($effect['target'] ?? ''),
            'io' => '2:io',
            'unsafe' => '3:unsafe',
            'panics' => '4:panics',
            'unresolved_call' => '5:unresolved_call:' . ($effect['name'] ?? ''),
            'opaque_loop' => '6:opaque_loop:' . ($effect['loopCid'] ?? ''),
            default => '99:' . Jcs::encode($effect),
        };
    }
}
