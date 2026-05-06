<?php
// ForwardPropagator — accumulate posts and emit implication-check diagnostics.
// Per: docs/lsp/forward-propagation-floor-v1.md

class ForwardPropagator {
    private array $seedCatalog = [];

    public function addToCatalog(string $calleeId, Post $pre, Post $post): void {
        $this->seedCatalog[$calleeId] = $post;
    }

    public function checkCallsite(string $calleeId, Post $currentPost): ?DiagnosticResult {
        if ($currentPost->isTop) return null;
        $calleePre = $this->seedCatalog[$calleeId] ?? null;
        if ($calleePre === null) return null;

        foreach ($currentPost->constraints as $c) {
            if (!in_array($c, $calleePre->constraints, true)) {
                return new DiagnosticResult(
                    "implication-failed",
                    "post does not imply callee pre: " . implode(" && ", $calleePre->constraints)
                );
            }
        }
        return null;
    }
}

class Post {
    public function __construct(
        public array $constraints,
        public bool $isTop
    ) {}

    public static function top(): Post {
        return new Post([], true);
    }

    public static function of(string $constraint): Post {
        return new Post([$constraint], false);
    }
}

class DiagnosticResult {
    public function __construct(
        public string $code,
        public string $message
    ) {}
}