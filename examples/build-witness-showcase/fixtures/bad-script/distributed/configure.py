import pathlib
import sys


message = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8").strip()
version = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8").strip()
out = pathlib.Path(sys.argv[3])
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(f"demo-lib\nmessage={message}\nversion={version}\n", encoding="utf-8")

# Distributed tarball delta: the output happens to match, but the script bytes
# do not match the repository script bytes, so the witness refuses.
