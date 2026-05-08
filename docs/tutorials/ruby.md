# Tutorial: Ruby

> **Status:** kit shipping in the current v1.6.2 tree. Lift adapters shipping: `active_model`, `dry-validation`, `rspec`. LSP plugin shipping. Verification via the Rust CLI. Requires Ruby 3+ (uses endless-method syntax).

A walkthrough for Ruby developers. By the end you have a `.proof` catalog lifted from existing `validates :field, presence: true`, `Dry::Validation::Contract`, or RSpec matchers.

## 1. What you'll have at the end

- A `.proof` file alongside your gem.
- Mementos derived from `validates`, dry-validation rules, or RSpec `it { is_expected.to ... }` matchers.
- LSP-driven squigglies in your editor.

## 2. Prerequisites

- Ruby 3+ (macOS system Ruby 2.6 cannot parse the kit; conformance harness prefers Homebrew Ruby).
- Rust toolchain on `PATH` (verifier subprocess).
- Z3 on `PATH` (Tier 3 only).

## 3. Install

```bash
cargo install --path implementations/rust/provekit-cli
provekit verify-protocol

cd implementations/ruby && bundle install
```

The Ruby kit lives at [implementations/ruby/](../../implementations/ruby/). There is no RubyGems package in the current source-built distribution.

## 4. Lift your first contract

ActiveModel:

```ruby
class User
  include ActiveModel::Validations
  validates :email, presence: true, format: { with: /\A[^@]+@[^@]+\.[^@]+\z/ }
  validates :age, numericality: { greater_than_or_equal_to: 0, less_than_or_equal_to: 150 }
end
```

Or dry-validation:

```ruby
class UserContract < Dry::Validation::Contract
  params do
    required(:email).filled(:string)
    required(:age).filled(:integer, gteq?: 0, lteq?: 150)
  end
end
```

Or RSpec:

```ruby
RSpec.describe User do
  it { is_expected.to validate_presence_of(:email) }
  it { is_expected.to validate_inclusion_of(:age).in_range(0..150) }
end
```

Run the lifter:

```bash
bundle exec provekit-lift-ruby
```

## 5. Verify

```bash
provekit prove
```

## 6. Wire your IDE and CI

- **IDE:** install the LSP plugin (`bin/provekit-lsp-ruby`). See [docs/how-to/ide-integration/](../how-to/ide-integration/).
- **CI:** see [content-addressed CI](../how-to/content-addressed-ci.md).

## What's next

- [docs/how-to/publishing-a-proof.md](../how-to/publishing-a-proof.md): ship the `.proof` alongside your gem.
- [docs/how-to/cross-domain-bridges.md](../how-to/cross-domain-bridges.md).
- [docs/reference/per-adapter-coverage.md](../reference/per-adapter-coverage.md).
- [docs/explanation/thesis.md](../explanation/thesis.md).

---

*This tutorial is a stub. Known gap: Bridge IR currently hardcodes `kind: "contract"` (task #190), blocking Phase 2 cross-kit bridges.*
