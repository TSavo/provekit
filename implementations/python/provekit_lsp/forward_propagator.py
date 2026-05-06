"""ForwardPropagator — accumulate posts and emit implication-check diagnostics.
Per: docs/lsp/forward-propagation-floor-v1.md
"""

from dataclasses import dataclass
from typing import Optional


@dataclass
class Post:
    constraints: list[str]
    is_top: bool

    @staticmethod
    def top() -> "Post":
        return Post(constraints=[], is_top=True)

    @staticmethod
    def of(constraint: str) -> "Post":
        return Post(constraints=[constraint], is_top=False)


@dataclass
class DiagnosticResult:
    code: str
    message: str


class ForwardPropagator:
    def __init__(self):
        self._seed_catalog: dict[str, Post] = {}

    def add_to_catalog(self, callee_id: str, pre: Post, post: Post) -> None:
        self._seed_catalog[callee_id] = post

    def check_callsite(
        self, callee_id: str, current_post: Post
    ) -> Optional[DiagnosticResult]:
        if current_post.is_top:
            return None
        callee_pre = self._seed_catalog.get(callee_id)
        if callee_pre is None:
            return None
        for c in current_post.constraints:
            if c not in callee_pre.constraints:
                return DiagnosticResult(
                    code="implication-failed",
                    message=f"post does not imply callee pre: {' && '.join(callee_pre.constraints)}",
                )
        return None
