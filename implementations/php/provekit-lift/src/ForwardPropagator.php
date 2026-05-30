<?php
declare(strict_types=1);

require_once __DIR__ . '/../../provekit-ir-symbolic/src/Canonicalizer/Blake3.php';

use ProvekIt\Canonicalizer\Blake3;

const PROVEKIT_LSP_PROTOCOL_CATALOG_CID = 'blake3-512:0e3905c2a7a098cd538b9669428a7dffd2b84ba8ccf8fde3724fe2ab61fd3fbc1e1a616a6b20b6817464cdc50c466b5497d4ac2e2dc34c3c15f05535b463643c';
const PROVEKIT_LSP_IMPLICATION_FAILED_CODE = 'provekit.lsp.implication_failed';

final class ForwardPost
{
    /** @param string[] $constraints */
    public function __construct(
        public array $constraints = [],
        public bool $isTop = false,
    ) {
        if (!$this->isTop) {
            $this->constraints = self::normalizeConstraints($this->constraints);
        } else {
            $this->constraints = [];
        }
    }

    /** @param string[] $constraints */
    public static function known(array $constraints): self
    {
        return new self($constraints, false);
    }

    public static function empty(): self
    {
        return new self([], false);
    }

    public static function top(): self
    {
        return new self([], true);
    }

    public function combine(self $next): self
    {
        if ($this->isTop || $next->isTop) {
            return self::top();
        }
        return self::known(array_merge($this->constraints, $next->constraints));
    }

    public function branchMerge(self $other): self
    {
        if ($this->isTop || $other->isTop) {
            return self::top();
        }
        $otherSet = array_fill_keys($other->constraints, true);
        $shared = array_values(array_filter(
            $this->constraints,
            fn(string $constraint): bool => isset($otherSet[$constraint]),
        ));
        return self::known($shared);
    }

    public function cid(): string
    {
        if ($this->isTop) {
            return Blake3::cid('post:top');
        }
        return Blake3::cid('post:known:' . implode("\n", $this->constraints));
    }

    /** @param string[] $constraints */
    private static function normalizeConstraints(array $constraints): array
    {
        $set = [];
        foreach ($constraints as $constraint) {
            if ($constraint !== '') {
                $set[$constraint] = true;
            }
        }
        $values = array_keys($set);
        sort($values, SORT_STRING);
        return $values;
    }
}

final class LspPosition
{
    public function __construct(
        public int $line,
        public int $character,
    ) {}

    /** @return array{line: int, character: int} */
    public function toArray(): array
    {
        return ['line' => $this->line, 'character' => $this->character];
    }
}

final class LspRange
{
    public function __construct(
        public LspPosition $start,
        public LspPosition $end,
    ) {}

    public static function singleLine(int $line, int $startCharacter, int $endCharacter): self
    {
        return new self(
            new LspPosition($line, $startCharacter),
            new LspPosition($line, $endCharacter),
        );
    }

    /** @return array{start: array, end: array} */
    public function toArray(): array
    {
        return ['start' => $this->start->toArray(), 'end' => $this->end->toArray()];
    }
}

final class BaselineEntry
{
    public function __construct(
        public string $calleeId,
        public ?ForwardPost $pre,
        public ?ForwardPost $post,
        public string $contractName,
        public string $memberCid,
        public string $contractCid,
        public string $attestationCid,
        public string $preCid,
        public string $postCid,
        public string $signer,
        public string $signerRole,
        public string $baselineCatalogCid,
        public string $baselineContractSetCid,
        public string $baselineIndexCid,
        public string $protocolCatalogCid,
    ) {}

    public static function new(string $calleeId, ?ForwardPost $pre, ?ForwardPost $post): self
    {
        $preCid = $pre ? $pre->cid() : Blake3::cid($calleeId . ':pre:none');
        $postCid = $post ? $post->cid() : Blake3::cid($calleeId . ':post:none');
        $seed = $calleeId . '|' . $preCid . '|' . $postCid;

        return new self(
            calleeId: $calleeId,
            pre: $pre,
            post: $post,
            contractName: 'php_baseline_' . self::sanitizeIdentifier($calleeId),
            memberCid: Blake3::cid('member:' . $seed),
            contractCid: Blake3::cid('contract:' . $seed),
            attestationCid: Blake3::cid('attestation:' . $seed),
            preCid: $preCid,
            postCid: $postCid,
            signer: 'ed25519:foundation-v0',
            signerRole: 'foundation-baseline',
            baselineCatalogCid: Blake3::cid('baseline-catalog:' . $seed),
            baselineContractSetCid: Blake3::cid('baseline-contract-set:' . $seed),
            baselineIndexCid: Blake3::cid('baseline-index:' . $seed),
            protocolCatalogCid: PROVEKIT_LSP_PROTOCOL_CATALOG_CID,
        );
    }

    private static function sanitizeIdentifier(string $value): string
    {
        $sanitized = preg_replace('/[^A-Za-z0-9]/', '_', $value);
        return is_string($sanitized) && $sanitized !== '' ? $sanitized : 'unknown';
    }
}

final class ForwardStmt
{
    private function __construct(
        public string $kind,
        public ?ForwardPost $post = null,
        public ?string $calleeId = null,
        public ?LspRange $range = null,
        public array $thenBranch = [],
        public array $elseBranch = [],
    ) {}

    public static function reset(): self
    {
        return new self('reset');
    }

    public static function assign(ForwardPost $post): self
    {
        return new self('assign', post: $post);
    }

    public static function call(string $calleeId, LspRange $range): self
    {
        return new self('call', calleeId: $calleeId, range: $range);
    }

    /** @param self[] $thenBranch @param self[] $elseBranch */
    public static function ifElse(array $thenBranch, array $elseBranch): self
    {
        return new self('if_else', thenBranch: $thenBranch, elseBranch: $elseBranch);
    }

    public static function unsupported(): self
    {
        return new self('unsupported');
    }
}

final class LspDiagnostic
{
    /** @param string[] $missingConjuncts */
    public function __construct(
        public LspRange $range,
        public BaselineEntry $entry,
        public ForwardPost $currentPost,
        public array $missingConjuncts,
    ) {}

    /** @return array<string, mixed> */
    public function toArray(): array
    {
        return [
            'range' => $this->range->toArray(),
            'severity' => 1,
            'source' => 'provekit',
            'code' => PROVEKIT_LSP_IMPLICATION_FAILED_CODE,
            'message' => 'callee precondition not established at this callsite',
            'data' => [
                'schema_version' => 1,
                'kind' => PROVEKIT_LSP_IMPLICATION_FAILED_CODE,
                'callee' => $this->entry->calleeId,
                'callee_contract_cid' => $this->entry->contractCid,
                'callee_attestation_cid' => $this->entry->attestationCid,
                'callee_pre_cid' => $this->entry->preCid,
                'callee_post_cid' => $this->entry->postCid,
                'current_post_cid' => $this->currentPost->cid(),
                'missing_conjuncts' => $this->missingConjuncts,
                'signer' => $this->entry->signer,
                'signer_role' => $this->entry->signerRole,
                'baseline_catalog_cid' => $this->entry->baselineCatalogCid,
                'baseline_contract_set_cid' => $this->entry->baselineContractSetCid,
                'baseline_index_cid' => $this->entry->baselineIndexCid,
                'protocol_catalog_cid' => $this->entry->protocolCatalogCid,
            ],
        ];
    }
}

final class ForwardPropagator
{
    /** @var array<string, BaselineEntry> */
    private array $index = [];

    /** @param BaselineEntry[] $entries */
    public function __construct(array $entries)
    {
        foreach ($entries as $entry) {
            $this->index[$entry->calleeId] = $entry;
        }
    }

    public static function floorV1SeedIndex(): self
    {
        return new self([
            BaselineEntry::new(
                '\\checkPositive',
                ForwardPost::known(['x > 0']),
                ForwardPost::known(['returns true']),
            ),
        ]);
    }

    /** @param ForwardStmt[] $body @return LspDiagnostic[] */
    public function emitDiagnostics(array $body): array
    {
        $diagnostics = [];
        $this->walkBlock($body, ForwardPost::empty(), $diagnostics);
        return $diagnostics;
    }

    public function checkCallsite(string $calleeId, ForwardPost $currentPost, LspRange $range): ?LspDiagnostic
    {
        if ($currentPost->isTop) {
            return null;
        }
        $entry = $this->index[$calleeId] ?? null;
        if (!$entry || !$entry->pre) {
            return null;
        }
        $currentSet = array_fill_keys($currentPost->constraints, true);
        $missing = [];
        foreach ($entry->pre->constraints as $constraint) {
            if (!isset($currentSet[$constraint])) {
                $missing[] = $constraint;
            }
        }
        if ($missing === []) {
            return null;
        }
        return new LspDiagnostic($range, $entry, $currentPost, $missing);
    }

    /** @param ForwardStmt[] $body @param LspDiagnostic[] $diagnostics */
    private function walkBlock(array $body, ForwardPost $startPost, array &$diagnostics): ForwardPost
    {
        $currentPost = $startPost;
        foreach ($body as $stmt) {
            switch ($stmt->kind) {
                case 'reset':
                    $currentPost = ForwardPost::empty();
                    break;
                case 'assign':
                    $currentPost = $currentPost->combine($stmt->post ?? ForwardPost::empty());
                    break;
                case 'call':
                    if ($stmt->calleeId !== null && $stmt->range !== null) {
                        $diagnostic = $this->checkCallsite($stmt->calleeId, $currentPost, $stmt->range);
                        if ($diagnostic) {
                            $diagnostics[] = $diagnostic;
                            break;
                        }
                        $entry = $this->index[$stmt->calleeId] ?? null;
                        if ($entry && $entry->post) {
                            $currentPost = $currentPost->combine($entry->post);
                        } elseif (!$entry) {
                            $currentPost = ForwardPost::top();
                        }
                    }
                    break;
                case 'if_else':
                    $thenPost = $this->walkBlock($stmt->thenBranch, $currentPost, $diagnostics);
                    $elsePost = $this->walkBlock($stmt->elseBranch, $currentPost, $diagnostics);
                    $currentPost = $thenPost->branchMerge($elsePost);
                    break;
                case 'unsupported':
                    $currentPost = ForwardPost::top();
                    break;
            }
        }
        return $currentPost;
    }

    /** @return ForwardStmt[] */
    public static function lowerFloorSource(string $source): array
    {
        $stmts = [];
        $braceDepth = 0;
        $topBlockDepth = null;
        $topSingleStatementPending = false;
        $scanLines = explode("\n", self::maskNonCode($source));

        foreach (explode("\n", $source) as $lineIdx => $line) {
            $scanLine = $scanLines[$lineIdx] ?? '';
            $trimmed = ltrim($scanLine);
            $isFunctionDefinition = self::isFunctionDefinition($trimmed);
            if ($isFunctionDefinition) {
                $stmts[] = ForwardStmt::reset();
                $topBlockDepth = null;
                $topSingleStatementPending = false;
            }

            $startsTopFallbackBlock = self::startsTopFallbackBlock($trimmed);
            if ($startsTopFallbackBlock) {
                $depth = $braceDepth + substr_count($scanLine, '{') - substr_count($scanLine, '}');
                if ($depth > $braceDepth) {
                    $topBlockDepth = $depth;
                } else {
                    $topSingleStatementPending = true;
                }
            }
            if (!$isFunctionDefinition && !$startsTopFallbackBlock && $topSingleStatementPending && str_contains($scanLine, '{')) {
                $depth = $braceDepth + substr_count($scanLine, '{') - substr_count($scanLine, '}');
                if ($depth <= $braceDepth) {
                    $depth = $braceDepth + 1;
                }
                $topBlockDepth = $depth;
                $topSingleStatementPending = false;
            }

            $calls = self::checkPositiveCalls($scanLine);
            if (!$isFunctionDefinition) {
                foreach ($calls as [$rangeStart, $nameStart, $arg]) {
                    if ($topBlockDepth !== null || $topSingleStatementPending) {
                        $stmts[] = ForwardStmt::unsupported();
                    } else {
                        $stmts[] = ForwardStmt::assign(self::postForCheckPositiveArg($arg));
                    }
                    $stmts[] = ForwardStmt::call(
                        '\\checkPositive',
                        LspRange::singleLine($lineIdx, $rangeStart, $nameStart + strlen('checkPositive')),
                    );
                }
            }

            $braceDepth += substr_count($scanLine, '{');
            $braceDepth -= substr_count($scanLine, '}');
            if ($topBlockDepth !== null && $braceDepth < $topBlockDepth) {
                $topBlockDepth = null;
            }
            if ($topSingleStatementPending) {
                $lineConsumesPendingStatement = !$startsTopFallbackBlock
                    && trim($trimmed) !== ''
                    && trim($trimmed) !== '{'
                    && trim($trimmed) !== '}';
                if ($lineConsumesPendingStatement || $calls !== [] || self::topHeaderHasInlineStatement($trimmed)) {
                    $topSingleStatementPending = false;
                }
            }
        }

        return $stmts;
    }

    private static function isFunctionDefinition(string $trimmed): bool
    {
        return preg_match('/^(?:(?:public|protected|private|static|final|abstract)\s+)*function\s+/', $trimmed) === 1;
    }

    private static function startsTopFallbackBlock(string $trimmed): bool
    {
        return str_starts_with($trimmed, 'for ') || str_starts_with($trimmed, 'for(')
            || str_starts_with($trimmed, 'while ') || str_starts_with($trimmed, 'while(')
            || str_starts_with($trimmed, 'foreach ') || str_starts_with($trimmed, 'foreach(')
            || str_starts_with($trimmed, 'switch ') || str_starts_with($trimmed, 'switch(')
            || $trimmed === 'do' || str_starts_with($trimmed, 'do ') || str_starts_with($trimmed, 'do{');
    }

    private static function topHeaderHasInlineStatement(string $trimmed): bool
    {
        $open = strpos($trimmed, '(');
        if ($open === false) {
            return false;
        }

        $depth = 0;
        $length = strlen($trimmed);
        for ($idx = $open; $idx < $length; $idx++) {
            $char = $trimmed[$idx];
            if ($char === '(') {
                $depth++;
            } elseif ($char === ')') {
                $depth--;
                if ($depth === 0) {
                    $trailing = trim(substr($trimmed, $idx + 1));
                    return $trailing !== '' && $trailing !== '{';
                }
            }
        }

        return false;
    }

    /** @return array<int, array{0: int, 1: int, 2: string}> */
    private static function checkPositiveCalls(string $line): array
    {
        $calls = [];
        $name = 'checkPositive';
        $nameLen = strlen($name);
        $lineLen = strlen($line);
        $searchFrom = 0;
        while (($relativeStart = strpos($line, $name, $searchFrom)) !== false) {
            $start = $relativeStart;
            if ($start > 0) {
                if (self::isIdentifierByte($line[$start - 1]) || self::hasQualifiedCallPrefix($line, $start)) {
                    $searchFrom = $start + $nameLen;
                    continue;
                }
            }

            $cursor = $start + $nameLen;
            if ($cursor < $lineLen && self::isIdentifierByte($line[$cursor])) {
                $searchFrom = $cursor;
                continue;
            }
            while ($cursor < $lineLen && ($line[$cursor] === ' ' || $line[$cursor] === "\t")) {
                $cursor++;
            }
            if ($cursor >= $lineLen || $line[$cursor] !== '(') {
                $searchFrom = $start + $nameLen;
                continue;
            }

            $argsStart = $cursor + 1;
            $depth = 1;
            $end = $argsStart;
            while ($end < $lineLen) {
                if ($line[$end] === '(') {
                    $depth++;
                } elseif ($line[$end] === ')') {
                    $depth--;
                    if ($depth === 0) {
                        break;
                    }
                }
                $end++;
            }
            if ($end >= $lineLen || $depth !== 0) {
                break;
            }
            $rangeStart = ($start > 0 && $line[$start - 1] === '\\') ? $start - 1 : $start;
            $calls[] = [$rangeStart, $start, trim(substr($line, $argsStart, $end - $argsStart))];
            $searchFrom = $end + 1;
        }
        return $calls;
    }

    private static function isIdentifierByte(string $char): bool
    {
        return $char === '$' || $char === '_' || ctype_alnum($char);
    }

    private static function hasQualifiedCallPrefix(string $line, int $start): bool
    {
        for ($idx = $start - 1; $idx >= 0; $idx--) {
            if ($line[$idx] === ' ' || $line[$idx] === "\t") {
                continue;
            }
            if ($line[$idx] === '>') {
                return $idx > 0 && $line[$idx - 1] === '-';
            }
            if ($line[$idx] === ':') {
                return $idx > 0 && $line[$idx - 1] === ':';
            }
            if ($line[$idx] === '\\') {
                return $idx > 0 && self::isIdentifierByte($line[$idx - 1]);
            }
            return false;
        }
        return false;
    }

    private static function maskNonCode(string $source): string
    {
        $masked = '';
        foreach (token_get_all($source) as $token) {
            if (is_array($token)) {
                [$id, $text] = $token;
                if (in_array($id, [
                    T_COMMENT,
                    T_DOC_COMMENT,
                    T_CONSTANT_ENCAPSED_STRING,
                    T_ENCAPSED_AND_WHITESPACE,
                    T_INLINE_HTML,
                    T_START_HEREDOC,
                    T_END_HEREDOC,
                ], true)) {
                    $masked .= self::maskTokenText($text);
                } else {
                    $masked .= $text;
                }
            } else {
                $masked .= $token;
            }
        }
        return $masked;
    }

    private static function maskTokenText(string $text): string
    {
        return preg_replace('/[^\r\n]/', ' ', $text) ?? '';
    }

    private static function postForCheckPositiveArg(string $arg): ForwardPost
    {
        if (!preg_match('/^-?\d+$/', $arg)) {
            return ForwardPost::top();
        }
        return (int)$arg > 0 ? ForwardPost::known(['x > 0']) : ForwardPost::known(['x <= 0']);
    }
}
