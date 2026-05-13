# Python Language Signature Memento

This catalog describes the draft Python source operation algebra emitted by
`provekit-lift-python-source`.

Version: `0.1.0-draft`

The carrier is `FunctionContractMemento` over statement and expression terms.
Every operation name is `python:`-prefixed. Unsupported Python syntax is refused
by the lifter instead of being represented by an unknown or fallback operation.

Core operations include `python:source-unit`, `python:seq`, `python:assign`,
`python:if`, `python:while`, `python:for`, `python:return`, arithmetic and
bitwise operators, `python:and` and `python:or` with short-circuit right-hand
slots, `python:compare`, `python:call`, `python:attribute`, and
`python:subscript`.
