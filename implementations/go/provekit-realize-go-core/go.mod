module github.com/tsavo/provekit/go/provekit-realize-go-core

go 1.22

require github.com/tsavo/provekit/go/provekit-ir-symbolic v0.0.0

require (
	github.com/klauspost/cpuid/v2 v2.0.9 // indirect
	lukechampine.com/blake3 v1.4.1 // indirect
)

replace github.com/tsavo/provekit/go/provekit-ir-symbolic => ../provekit-ir-symbolic
