# Emacs integration

Emacs has multiple LSP frontends; Sugar LSPs work with all of them. This doc covers `lsp-mode` (most common) and `eglot` (built-in, Emacs 29+).

## lsp-mode

Add Sugar LSPs to your `lsp-mode` configuration:

```elisp
(use-package lsp-mode
  :init
  (setq lsp-keymap-prefix "C-c l")
  :commands lsp
  :config
  ;; Register each Sugar LSP
  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection "sugar-lsp-rust")
      :major-modes '(rust-mode)
      :priority 0
      :server-id 'sugar-rust
      :initialization-options '(:sugar (:protocolVersion "1.1.0"))))

  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection "sugar-lsp-py")
      :major-modes '(python-mode)
      :priority 0
      :server-id 'sugar-python
      :initialization-options '(:sugar (:protocolVersion "1.1.0"))))

  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection (lambda () (list "sugar-lift-zig" "--rpc")))
      :major-modes '(zig-mode)
      :priority 0
      :server-id 'sugar-zig
      :initialization-options '(:sugar (:protocolVersion "1.1.0"))))

  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection "sugar-lsp-ruby")
      :major-modes '(ruby-mode)
      :priority 0
      :server-id 'sugar-ruby
      :initialization-options '(:sugar (:protocolVersion "1.1.0"))))

  (lsp-register-client
    (make-lsp-client
      :new-connection (lsp-stdio-connection "sugar-lsp-csharp")
      :major-modes '(csharp-mode)
      :priority 0
      :server-id 'sugar-csharp
      :initialization-options '(:sugar (:protocolVersion "1.1.0"))))
)
```

`priority 0` keeps the Sugar LSP at the same level as your primary language LSP (rust-analyzer, pylsp, etc.); both run; both publish diagnostics.

## eglot (Emacs 29+)

```elisp
(use-package eglot
  :config
  (add-to-list 'eglot-server-programs
               '(rust-mode . ("sugar-lsp-rust")))
  (add-to-list 'eglot-server-programs
               '(python-mode . ("sugar-lsp-py")))
  (add-to-list 'eglot-server-programs
               '(zig-mode . ("sugar-lift-zig" "--rpc")))
  (add-to-list 'eglot-server-programs
               '(ruby-mode . ("sugar-lsp-ruby")))
  (add-to-list 'eglot-server-programs
               '(csharp-mode . ("sugar-lsp-csharp"))))
```

Note: eglot only manages one LSP per buffer. To run Sugar alongside another LSP, use `lsp-mode` or compose via a wrapper.

## Diagnostics display

Both `lsp-mode` and `eglot` use Emacs's standard `flymake` or `flycheck` for diagnostics. Configure the visual style:

```elisp
;; lsp-mode + flycheck
(use-package flycheck
  :init (global-flycheck-mode))

;; or for eglot + flymake (built-in)
(setq eglot-stay-out-of '(eldoc-documentation-functions))
```

Sugar diagnostics will appear in the modeline (`flycheck-mode-line`) and inline (with `flycheck-list-errors` to see the full list).

## Filter to Sugar only

```elisp
(defun my/sugar-only-errors ()
  "Show only Sugar errors in the current buffer."
  (interactive)
  (let ((errors (flycheck-overlay-errors-in (point-min) (point-max))))
    (let ((sugar-errors
           (cl-remove-if-not
            (lambda (e) (string= "sugar" (flycheck-error-checker e)))
            errors)))
      (with-output-to-temp-buffer "*Sugar Errors*"
        (dolist (e sugar-errors)
          (princ (format "%s\n" (flycheck-error-message e))))))))
```

## Performance tuning

```elisp
;; Lower Tier 3 timeout
(setq lsp-sugar-tier3-timeout 2000)

;; Disable LSP for huge buffers
(setq lsp-file-watch-threshold 1000)
(setq lsp-idle-delay 0.5)
```

## Configuring per-project

Use `.dir-locals.el` for per-project configuration:

```elisp
((rust-mode . ((lsp-sugar-tier3-timeout . 5000)
               (lsp-sugar-protocol-version . "1.1.0"))))
```

## Troubleshooting

### LSP doesn't start

- `M-x lsp-describe-session` (lsp-mode) shows registered LSPs.
- `M-x lsp-toggle-trace-io` enables verbose logging in `*lsp-log*` buffer.
- Verify `which sugar-lsp-rust` returns a path; if not, the binary isn't on PATH.
- Verify `sugar verify-protocol` works from a shell.

### Squigglies don't appear

- Check `*Messages*` buffer for LSP errors.
- Confirm `lsp-mode` is active in the buffer (`C-h v lsp-mode-active`).
- Run `M-x flycheck-list-errors` to see all diagnostics; if Sugar isn't a source, it didn't connect.

### LSP repeatedly restarts

- Check `*lsp-log*` for crashes.
- Common cause: protocol version mismatch. Verify with `sugar verify-protocol`.

### Slow performance

- Lower `lsp-sugar-tier3-timeout`.
- Increase `lsp-idle-delay` to 1.0 (parses less often).
- Check if Tier 3 is dominating; if so, the lattice is cold.

## Per-mode specifics

### Rust

Run alongside `rust-analyzer` (configured via `lsp-mode` with `rust-analyzer` as the primary LSP and `sugar-rust` as secondary).

### Python

Run alongside `pylsp` or `pyright`. Both publish diagnostics; both work concurrently.

### Zig

Pairs with `zls`; both run.

### Ruby

Pairs with `solargraph` (when available); both run.

### C#

Pairs with `omnisharp-emacs` or `csharp-ls`.

## Read next

- [overview.md](overview.md).
- [vscode.md](vscode.md): VSCode equivalent.
- [neovim.md](neovim.md): Neovim equivalent.
- [jetbrains.md](jetbrains.md): JetBrains equivalent.
