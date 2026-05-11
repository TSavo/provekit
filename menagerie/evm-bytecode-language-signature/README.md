# EVM Bytecode Language Signature

This menagerie entry names the EVM bytecode primitive surface used by
`provekit-lift-evm-bytecode`.

The signature is bytecode-domain first: byte offsets carry program order, stack
operator slots preserve operand-pop order, and no Solidity or Vyper source terms
appear in the primitive vocabulary. Solidity, Vyper, Huff, Yul, and other
Ethereum contract languages can target this surface by compiling to EVM bytecode
and linking by `::Path`.

The initial executable slice covers deterministic straight-line stack programs:
`PUSH0`, `PUSH1` through `PUSH32`, arithmetic, comparison, and bitwise stack
operators, `POP`, `DUP`, `SWAP`, `JUMPDEST`, and `STOP`. `RETURN`, dynamic
control flow, storage, logs, contract creation, external calls, and exceptional
exits are declared but refused explicitly until their effects are modeled in the
catalog and lifter output.
