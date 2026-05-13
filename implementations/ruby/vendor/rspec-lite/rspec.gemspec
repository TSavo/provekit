Gem::Specification.new do |spec|
  spec.name = "rspec"
  spec.version = "3.13.0"
  spec.summary = "Small local RSpec-compatible runner for sandboxed ProvekIt tests"
  spec.authors = ["ProvekIt"]
  spec.files = Dir["lib/**/*.rb"] + Dir["bin/*"]
  spec.bindir = "bin"
  spec.executables = ["rspec"]
  spec.required_ruby_version = ">= 2.6"
end
