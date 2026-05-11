# EVM Bytecode Language Signature

This menagerie entry names the EVM bytecode primitive surface used by
`provekit-lift-evm-bytecode`.

The signature is bytecode-domain first: instruction streams are positional,
program counters are byte offsets, stack positions are address-like child
edges, and no Solidity or Vyper source terms appear in the primitive
vocabulary. Solidity, Vyper, Huff, Yul, and other Ethereum contract languages
can target this surface by compiling to EVM bytecode and linking by `::Path`.

The initial executable slice covers deterministic straight-line stack programs:
`PUSH0`, `PUSH1` through `PUSH32`, arithmetic and comparison stack operators,
`POP`, `DUP`, `SWAP`, `JUMPDEST`, `STOP`, and `RETURN`. Dynamic control flow,
storage, logs, contract creation, external calls, and exceptional exits are
refused explicitly until their effects are modeled in the catalog.
