<?php
/** ProvekIt IR — Declaration types and collector pattern. */

namespace ProvekIt\Ir;

class ContractDecl implements \JsonSerializable {
    public function __construct(
        public readonly string $name,
        public readonly string $outBinding = 'out',
        public readonly ?IrFormula $pre = null,
        public readonly ?IrFormula $post = null,
        public readonly ?IrFormula $inv = null,
    ) {}

    public function jsonSerialize(): array {
        $r = ['kind' => 'contract', 'name' => $this->name, 'outBinding' => $this->outBinding];
        if ($this->pre !== null)  $r['pre'] = $this->pre->jsonSerialize();
        if ($this->post !== null) $r['post'] = $this->post->jsonSerialize();
        if ($this->inv !== null)  $r['inv'] = $this->inv->jsonSerialize();
        return $r;
    }
}

class BridgeDecl implements \JsonSerializable {
    public function __construct(
        public readonly string $name,
        public readonly string $sourceSymbol,
        public readonly string $sourceLayer,
        public readonly string $sourceContractCid,
        public readonly string $targetContractCid,
        public readonly string $targetProofCid,
        public readonly string $targetLayer,
        public readonly ?string $notes = null,
    ) {}

    public function jsonSerialize(): array {
        $r = [
            'kind' => 'bridge',
            'name' => $this->name,
            'sourceSymbol' => $this->sourceSymbol,
            'sourceLayer' => $this->sourceLayer,
            'sourceContractCid' => $this->sourceContractCid,
            'targetContractCid' => $this->targetContractCid,
            'targetProofCid' => $this->targetProofCid,
            'targetLayer' => $this->targetLayer,
        ];
        if ($this->notes !== null) $r['notes'] = $this->notes;
        return $r;
    }
}

// ---------- Collector (side-effect-based declaration collection) ----------

class Collector {
    /** @var ContractDecl[] */
    private static array $contracts = [];
    /** @var BridgeDecl[] */
    private static array $bridges = [];

    public static function reset(): void {
        self::$contracts = [];
        self::$bridges = [];
    }

    public static function Contract(
        string $name,
        ?IrFormula $pre = null,
        ?IrFormula $post = null,
        ?IrFormula $inv = null,
        string $outBinding = 'out',
    ): void {
        self::$contracts[] = new ContractDecl($name, $outBinding, $pre, $post, $inv);
    }

    /** Abbreviated: Must(name, formula) -> Contract(name, pre=formula) */
    public static function Must(string $name, IrFormula $pre): void {
        self::$contracts[] = new ContractDecl($name, 'out', $pre, null, null);
    }

    public static function Bridge(
        string $name,
        string $sourceSymbol,
        string $sourceLayer,
        string $sourceContractCid,
        string $targetContractCid,
        string $targetProofCid,
        string $targetLayer,
        ?string $notes = null,
    ): void {
        self::$bridges[] = new BridgeDecl(
            $name, $sourceSymbol, $sourceLayer,
            $sourceContractCid, $targetContractCid,
            $targetProofCid, $targetLayer, $notes
        );
    }

    /** @return array{contracts: ContractDecl[], bridges: BridgeDecl[]} */
    public static function finish(): array {
        $result = ['contracts' => self::$contracts, 'bridges' => self::$bridges];
        self::reset();
        return $result;
    }
}
