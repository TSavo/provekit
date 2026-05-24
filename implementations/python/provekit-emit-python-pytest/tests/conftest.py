from __future__ import annotations

import sys
from pathlib import Path

# Put this kit's src on the path so tests run without an editable install,
# matching the convention in the other python kits.
PKG_SRC = Path(__file__).resolve().parents[1] / "src"
if str(PKG_SRC) not in sys.path:
    sys.path.insert(0, str(PKG_SRC))
