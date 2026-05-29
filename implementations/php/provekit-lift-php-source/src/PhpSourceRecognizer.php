<?php

declare(strict_types=1);

namespace ProvekIt\LiftPhpSource;

use ProvekIt\Canonicalizer\Jcs;

final class PhpSourceRecognizer
{
    /**
     * @param array<int, string> $sourcePaths
     * @param array<int, array> $bindingTemplates
     * @return array{tags: array<int, array>}
     */
    public function recognizePaths(string $projectRoot, array $sourcePaths, array $bindingTemplates): array
    {
        $root = realpath($projectRoot);
        if ($root === false) {
            throw new \InvalidArgumentException('project_root not found: ' . $projectRoot);
        }

        $bindings = array_merge(
            $this->loadProjectBindingTemplates($root),
            array_values(array_filter($bindingTemplates, static fn(mixed $binding): bool => is_array($binding)))
        );
        $tags = [];
        $lifter = new PhpSourceLifter();

        foreach ($sourcePaths as $sourcePath) {
            if (!is_string($sourcePath)) {
                continue;
            }
            $fullPath = $this->resolveInsideRoot($root, $sourcePath);
            if ($fullPath === null || !is_file($fullPath)) {
                continue;
            }
            $source = file_get_contents($fullPath);
            if ($source === false) {
                continue;
            }

            $lifted = $lifter->liftSource($source, $sourcePath);
            foreach ($lifted['ir'] as $contract) {
                if (($contract['kind'] ?? null) !== 'function-contract') {
                    continue;
                }
                $fnName = $contract['fnName'] ?? null;
                if (!is_string($fnName) || str_starts_with($fnName, '<source-unit:')) {
                    continue;
                }
                $bodySource = $contract['body_source'] ?? null;
                if (!is_array($bodySource)) {
                    continue;
                }
                $binding = $this->matchingBinding($bodySource, $bindings);
                if ($binding === null) {
                    continue;
                }
                $tags[] = $this->tagForMatch($sourcePath, $fnName, $contract, $bodySource, $binding);
            }
        }

        return ['tags' => $tags];
    }

    /**
     * @return array<int, array>
     */
    private function loadProjectBindingTemplates(string $root): array
    {
        $templates = [];
        foreach ($this->projectTemplatePaths($root) as $path) {
            if (!is_file($path)) {
                continue;
            }
            $raw = file_get_contents($path);
            if ($raw === false) {
                continue;
            }
            try {
                $decoded = json_decode($raw, true, 512, JSON_THROW_ON_ERROR);
            } catch (\JsonException) {
                continue;
            }
            if (is_array($decoded)) {
                array_push($templates, ...$this->bindingTemplatesFromDecodedProjectData($decoded));
            }
        }
        return $templates;
    }

    /**
     * @return array<int, string>
     */
    private function projectTemplatePaths(string $root): array
    {
        return [
            $root . '/.provekit/lift/php-source/recognize-templates.json',
            $root . '/.provekit/lift/php-source/binding-templates.json',
        ];
    }

    /**
     * @return array<int, array>
     */
    private function bindingTemplatesFromDecodedProjectData(array $decoded): array
    {
        if (array_is_list($decoded)) {
            return array_values(array_filter($decoded, static fn(mixed $binding): bool => is_array($binding)));
        }

        $templates = [];
        if (is_array($decoded['binding_templates'] ?? null)) {
            array_push($templates, ...array_values(array_filter(
                $decoded['binding_templates'],
                static fn(mixed $binding): bool => is_array($binding)
            )));
        }
        if (is_array($decoded['members'] ?? null)) {
            foreach ($decoded['members'] as $member) {
                if (!is_array($member)) {
                    continue;
                }
                $template = $this->bindingTemplateFromProofMember($member);
                if ($template !== null) {
                    $templates[] = $template;
                }
            }
        }
        return $templates;
    }

    private function bindingTemplateFromProofMember(array $member): ?array
    {
        $record = $this->unwrapEnvelope($member);
        if (($record['kind'] ?? null) !== 'library-sugar-binding-entry') {
            return null;
        }
        $bodySource = is_array($record['body_source'] ?? null) ? $record['body_source'] : [];
        if (!array_key_exists('ast_template', $bodySource) || !is_string($bodySource['template_cid'] ?? null)) {
            return null;
        }

        return [
            'concept_name' => $record['concept_name'] ?? null,
            'library_tag' => $record['target_library_tag'] ?? ($record['library_tag'] ?? null),
            'family' => $record['family'] ?? null,
            'ast_template' => $bodySource['ast_template'],
            'template_cid' => $bodySource['template_cid'],
            'param_names' => $bodySource['param_names'] ?? null,
            'contract_cid' => $record['contract_cid'] ?? null,
            'source_function_name' => $record['source_function_name'] ?? null,
        ];
    }

    private function unwrapEnvelope(array $value): array
    {
        if (is_array($value['body'] ?? null) && (array_key_exists('schemaVersion', $value) || array_key_exists('header', $value))) {
            return $value['body'];
        }
        return $value;
    }

    /**
     * @param array<int, array> $bindings
     */
    private function matchingBinding(array $bodySource, array $bindings): ?array
    {
        foreach ($bindings as $binding) {
            if ($this->bindingMatchesCandidate($binding, $bodySource)) {
                return $binding;
            }
        }
        return null;
    }

    private function bindingMatchesCandidate(array $binding, array $bodySource): bool
    {
        $candidateCid = $bodySource['template_cid'] ?? null;
        $bindingCid = $this->bindingTemplateCid($binding);
        if (is_string($bindingCid) && $bindingCid !== $candidateCid) {
            return false;
        }

        $candidateTemplate = $bodySource['ast_template'] ?? null;
        $bindingTemplate = $this->bindingAstTemplate($binding);
        if ($bindingTemplate !== null && $candidateTemplate !== null) {
            return Jcs::encode($bindingTemplate) === Jcs::encode($candidateTemplate);
        }

        return is_string($bindingCid) && $bindingCid === $candidateCid;
    }

    private function tagForMatch(string $sourcePath, string $fnName, array $contract, array $bodySource, array $binding): array
    {
        return [
            'file' => $sourcePath,
            'span' => $this->spanForMatch($contract, $bodySource),
            'function_name' => $fnName,
            'concept_name' => $this->stringField($binding, 'concept_name'),
            'library_tag' => $this->stringField($binding, 'library_tag', $this->stringField($binding, 'target_library_tag')),
            'family' => $binding['family'] ?? null,
            'template_cid' => (string)($bodySource['template_cid'] ?? ''),
            'contract_cid' => $this->stringField($binding, 'contract_cid'),
            'match_tier' => 'exact',
            'param_bindings' => $this->paramBindings($bodySource['param_names'] ?? []),
        ];
    }

    private function spanForMatch(array $contract, array $bodySource): array
    {
        if (is_array($bodySource['span'] ?? null)) {
            return $bodySource['span'];
        }
        $locus = is_array($contract['locus'] ?? null) ? $contract['locus'] : [];
        $line = is_int($locus['line'] ?? null) ? $locus['line'] : 1;
        $col = is_int($locus['col'] ?? null) ? $locus['col'] : 1;
        return source_span($line, $col, $line, $col);
    }

    /**
     * @param array<int, string> $paramNames
     * @return array<int, array{index: int, source_text: string}>
     */
    private function paramBindings(array $paramNames): array
    {
        $bindings = [];
        foreach (array_values($paramNames) as $i => $name) {
            if (is_string($name)) {
                $bindings[] = ['index' => $i + 1, 'source_text' => $name];
            }
        }
        return $bindings;
    }

    private function bindingTemplateCid(array $binding): ?string
    {
        if (is_string($binding['template_cid'] ?? null)) {
            return $binding['template_cid'];
        }
        if (is_array($binding['body_source'] ?? null) && is_string($binding['body_source']['template_cid'] ?? null)) {
            return $binding['body_source']['template_cid'];
        }
        return null;
    }

    private function bindingAstTemplate(array $binding): mixed
    {
        if (array_key_exists('ast_template', $binding)) {
            return $binding['ast_template'];
        }
        if (is_array($binding['body_source'] ?? null) && array_key_exists('ast_template', $binding['body_source'])) {
            return $binding['body_source']['ast_template'];
        }
        return null;
    }

    private function stringField(array $value, string $key, string $default = ''): string
    {
        return is_string($value[$key] ?? null) ? $value[$key] : $default;
    }

    private function resolveInsideRoot(string $root, string $sourcePath): ?string
    {
        $candidate = $sourcePath;
        if (!$this->isAbsolutePath($candidate)) {
            $candidate = $root . DIRECTORY_SEPARATOR . $sourcePath;
        }
        $parent = is_dir($candidate) ? $candidate : dirname($candidate);
        $realParent = realpath($parent);
        if ($realParent === false) {
            return null;
        }
        $resolved = $realParent . DIRECTORY_SEPARATOR . basename($candidate);
        $rootPrefix = rtrim($root, DIRECTORY_SEPARATOR) . DIRECTORY_SEPARATOR;
        return $resolved === $root || str_starts_with($resolved, $rootPrefix) ? $resolved : null;
    }

    private function isAbsolutePath(string $path): bool
    {
        return str_starts_with($path, DIRECTORY_SEPARATOR) || preg_match('/^[A-Za-z]:[\\\\\\/]/', $path) === 1;
    }
}
