Gem::Specification.new do |spec|
  spec.name = "provekit-ruby"
  spec.version = "0.1.0"
  spec.summary = "Ruby source kit for ProvekIt"
  spec.authors = ["ProvekIt"]
  spec.files = Dir["lib/**/*.rb"] + Dir["bin/*"] + ["README.md", "manifest.json"]
  spec.bindir = "bin"
  spec.executables = ["provekit-ruby-plugin"]
  spec.required_ruby_version = ">= 2.6"
end
