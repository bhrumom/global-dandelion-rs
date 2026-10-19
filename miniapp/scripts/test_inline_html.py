#!/usr/bin/env python3
"""Android injects the bundle as a single HTML string, so the inline variant must be
self-contained: no relative module specifiers, no external stylesheet, no 'self'
origin in the CSP. This builds it from stub resources and checks those properties.
"""
import re
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from package import INLINE_REWRITES, inline_html  # noqa: E402

INDEX = """<!doctype html><html lang="zh-CN"><head>
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'none'; img-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'">
<link rel="stylesheet" href="app.css"></head><body><p id="status">x</p>
<script type="module" src="app.js"></script></body></html>"""

APP = """import { connectHost } from './host-tools.js';
import * as adapter from './pkg/dandelion_web_adapter.js';
import { wasmBytes } from './wasm-bytes.js';
const host = await connectHost();
globalThis.__test = { host, adapter, wasmBytes };
"""


def build(root: Path) -> Path:
    (root / "pkg").mkdir(parents=True)
    (root / "index.html").write_text(INDEX, encoding="utf-8")
    (root / "app.css").write_text("body{margin:0}", encoding="utf-8")
    (root / "app.js").write_text(APP, encoding="utf-8")
    (root / "host-tools.js").write_text("export const connectHost = async () => ({ available: false });\n",
                                        encoding="utf-8")
    (root / "wasm-bytes.js").write_text("export const wasmBytes = new Uint8Array([0]);\n", encoding="utf-8")
    (root / "pkg" / "dandelion_web_adapter.js").write_text("export const encode_packet = () => {};\n",
                                                           encoding="utf-8")
    target = root.parent / "inline.html"
    inline_html(root, target)
    return target


def check(html: str) -> None:
    assert '<script type="module" src="app.js">' not in html, "app.js must be inlined"
    assert '<link rel="stylesheet" href="app.css">' not in html, "app.css must be inlined"
    assert "<style>body{margin:0}</style>" in html, "stylesheet content missing"
    assert 'type="importmap"' in html, "module specifiers need an import map"
    for name in INLINE_REWRITES.values():
        assert f'"{name}": "data:text/javascript;base64,' in html, f"missing module {name}"
    # The injected document has no origin, so 'self' can never resolve there.
    assert "script-src 'self'" not in html, "inline CSP must not rely on 'self'"
    assert "script-src 'unsafe-inline' data: 'wasm-unsafe-eval'" in html, "inline CSP missing"
    # Every relative specifier in the inlined body must have been rewritten.
    body = html.split('<script type="module">', 1)[1]
    leftovers = re.findall(r"(?:from\s+|import\s*\(\s*)'(\.[^']*)'", body)
    assert not leftovers, f"unrewritten module specifiers: {leftovers}"


def check_real_sources() -> None:
    """Every relative import in the shipped page must have a declared rewrite."""
    web = Path(__file__).resolve().parents[1] / "web"
    for name in ("app.js", "host-tools.js"):
        source = (web / name).read_text(encoding="utf-8")
        specifiers = set(re.findall(r"from\s+'(\./[^']*)'", source))
        unknown = specifiers - set(INLINE_REWRITES)
        assert not unknown, f"{name} imports modules the inline build cannot rewrite: {unknown}"


def main() -> None:
    check_real_sources()
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp) / "resources"
        target = build(root)
        check(target.read_text(encoding="utf-8"))
        print(f"inline bundle ok: {target.stat().st_size} bytes, 3 modules inlined")


if __name__ == "__main__":
    main()
