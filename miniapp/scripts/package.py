#!/usr/bin/env python3
"""Prepare deterministic OMA resources. Signing is an external Registry action."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import shutil
import zipfile

ROOT = Path(__file__).resolve().parents[2]
DOMAIN = b"fabushi.fabapp.manifest.v1\0"
MIME = {".html": "text/html", ".css": "text/css", ".js": "text/javascript",
        ".cjs": "text/javascript", ".json": "application/json",
        ".toml": "text/plain", ".bin": "application/octet-stream",
        ".wasm": "application/wasm", ".txt": "text/plain"}
# Android injects the bundle as an HTML string, so relative module specifiers
# cannot resolve. Rewrite them to bare names served by an import map.
INLINE_REWRITES = {
    "./pkg/dandelion_web_adapter.js": "fabushi-adapter",
    "./wasm-bytes.js": "fabushi-wasm-bytes",
    "./host-tools.js": "fabushi-host-tools",
    "./composer.js": "fabushi-composer",
    "./globe.js": "fabushi-globe",
    "./links.js": "fabushi-links",
}
INLINE_CSP = (
    "default-src 'none'; script-src 'unsafe-inline' data: 'wasm-unsafe-eval'; "
    "style-src 'unsafe-inline'; img-src data:; connect-src 'none'; "
    "object-src 'none'; base-uri 'none'; form-action 'none'"
)


def canonical(value):
    # All schema and resource keys are ASCII; numbers are bounded integers.
    return json.dumps(value, sort_keys=True, ensure_ascii=False,
                      separators=(",", ":"), allow_nan=False).encode("utf-8")


def prepare(pkg, output, source_commit, version, platforms, native_dir=None):
    if len(source_commit) != 40 or any(c not in "0123456789abcdef" for c in source_commit):
        raise ValueError("source commit must be an exact SHA")
    if not platforms:
        raise ValueError("at least one platform is required")
    if output.exists():
        raise ValueError("output already exists; never overwrite package preparation")
    output.mkdir(parents=True)
    for item in (ROOT / "miniapp/web").iterdir():
        if item.is_file():
            shutil.copyfile(item, output / item.name)
    app_js = output / "app.js"
    app_js.write_text(app_js.read_text(encoding="utf-8").replace("__FABUSHI_APP_VERSION__", version), encoding="utf-8")
    html = output / 'index.html'
    html.write_text(html.read_text(encoding='utf-8').replace('__FABUSHI_APP_VERSION__', version), encoding='utf-8')
    (output / "pkg").mkdir()
    for name in ["dandelion_web_adapter.js", "dandelion_web_adapter_bg.wasm"]:
        shutil.copyfile(pkg / name, output / "pkg" / name)
    wasm = (pkg / "dandelion_web_adapter_bg.wasm").read_bytes()
    # JS module delivers local Wasm bytes without fetch/connect permission.
    (output / "wasm-bytes.js").write_text(
        "export const wasmBytes = Uint8Array.from(atob('" + base64.b64encode(wasm).decode("ascii")
        + "'), c => c.charCodeAt(0));\n", encoding="utf-8")
    shutil.copyfile(ROOT / "LICENSE-MIT", output / "LICENSE-MIT.txt")
    shutil.copyfile(ROOT / "LICENSE-APACHE", output / "LICENSE-APACHE.txt")
    shutil.copyfile(ROOT / "miniapp/wasm/dist/THIRD-PARTY-NOTICES.txt", output / "THIRD-PARTY-NOTICES.txt")
    shutil.copyfile(ROOT / "miniapp/wasm/Cargo.lock", output / "WebCargoLock.txt")
    (output / "BUILD-INFO.txt").write_text(
        f"source={source_commit}\nnative-base=aa6436315cde7b66868b0592d1c67b4590ebd1f9\n"
        f"kind=offline-web-preview\nlicense=MIT OR Apache-2.0\n"
        f"version={version}\nplatforms={','.join(platforms)}\n"
        "network-transfer=host-mediated\n", encoding="utf-8")
    files = {}
    if native_dir is not None:
        if platforms != ['desktop'] or version not in ('0.4.0', '0.4.1', '0.4.2', '0.4.3'):
            raise ValueError('native package requires reviewed Mac-only 0.4.0/0.4.1/0.4.2/0.4.3 release')
        (output / 'native').mkdir()
        info = output / 'BUILD-INFO.txt'
        info.write_text(info.read_text().replace('kind=offline-web-preview', 'kind=install-owned-native-macos'), encoding='utf-8')
        for name in ['worker.cjs', 'dandelion.bin', 'default.toml']:
            shutil.copyfile(native_dir / name, output / 'native' / name)
        binary = (output / 'native/dandelion.bin').read_bytes()
        if binary[:8] != bytes.fromhex('cffaedfe0c000001') or len(binary) > 64 * 1024 * 1024:
            raise ValueError('native runtime must be bounded arm64 Mach-O')
        (output / 'native-runtime.json').write_bytes(canonical({
            'schema': 'fabushi.native-runtime.v1', 'platform': 'darwin',
            'architecture': 'arm64', 'entrypoint': 'native/worker.cjs',
            'executables': ['native/dandelion.bin'],
            'hostCapabilities': ['file-picker', 'confirmation', 'package-update'],
        }))
    for path in sorted(output.rglob("*")):
        if not path.is_file():
            continue
        name = path.relative_to(output).as_posix()
        if not name.isascii() or path.suffix not in MIME:
            raise ValueError("unsupported resource name/MIME")
        data = path.read_bytes()
        files[name] = {"sha256": hashlib.sha256(data).hexdigest(),
                       "bytes": len(data), "mime": MIME[path.suffix]}
    manifest = {
        "manifestVersion": 1, "packageClass": "catalog",
        "appId": "org.fabushi.global-dandelion", "publisherId": "bhrumom",
        "version": version,
        # `platforms` is not part of fabapp v1: both the Marketplace signer and the
        # host reject unknown manifest fields. Platform scope lives in the
        # descriptor and in BUILD-INFO.txt instead.
        "displayName": "Global Dandelion 原生工作台" if native_dir else "Global Dandelion 离线工作台",
        "entrypoint": "index.html", "runtime": "web", "minHostApi": 2 if native_dir else 1,
        # Native Host tools are discovered and authorized at runtime through
        # fabushi.miniapp.tools.v1. The package manifest must not self-grant
        # native capabilities; the production signer and Host both reject
        # requested capabilities in a catalog fabapp.
        "offlineCore": True, "requestedCapabilities": ['native.runtime'] if native_dir else [],
        "networkPolicy": {"mode": "deny", "allowedOrigins": []}, "files": files,
    }
    (output / "manifest.unsigned.json").write_bytes(canonical(manifest))
    return manifest


def inline_html(resources, destination):
    """Single self-contained HTML for hosts that inject a string (Android)."""
    html = (resources / "index.html").read_text(encoding="utf-8")
    css = (resources / "app.css").read_text(encoding="utf-8")
    app = (resources / "app.js").read_text(encoding="utf-8")
    for old, new in INLINE_REWRITES.items():
        app = app.replace(f"'{old}'", f"'{new}'").replace(f'"{old}"', f'"{new}"')
    modules = {
        "fabushi-adapter": (resources / "pkg/dandelion_web_adapter.js").read_text(encoding="utf-8"),
        "fabushi-wasm-bytes": (resources / "wasm-bytes.js").read_text(encoding="utf-8"),
        "fabushi-host-tools": (resources / "host-tools.js").read_text(encoding="utf-8"),
    }
    # Module dependencies are rewritten before inlining, including nested imports.
    for filename, module in [('links.js', 'fabushi-links'), ('globe.js', 'fabushi-globe'), ('composer.js', 'fabushi-composer')]:
        source = (resources / filename).read_text(encoding='utf-8')
        for old, new in INLINE_REWRITES.items():
            source = source.replace(f"'{old}'", f"'{new}'").replace(f'"{old}"', f'"{new}"')
        modules[module] = source
    imports = {name: "data:text/javascript;base64," + base64.b64encode(src.encode("utf-8")).decode("ascii")
               for name, src in modules.items()}
    html = html.replace('<link rel="stylesheet" href="app.css">', f"<style>{css}</style>")
    html = html.replace('<script type="module" src="app.js"></script>',
                        '<script type="importmap">' + json.dumps({"imports": imports}) + "</script>\n"
                        '<script type="module">' + app + "</script>")
    # The injected variant cannot use 'self': it has no origin of its own.
    html = html.replace(
        '<meta http-equiv="Content-Security-Policy" content="default-src \'none\'; script-src \'self\' \'wasm-unsafe-eval\'; style-src \'self\'; connect-src \'none\'; img-src \'self\'; object-src \'none\'; base-uri \'none\'; form-action \'none\'">',
        f'<meta http-equiv="Content-Security-Policy" content="{INLINE_CSP}">')
    destination.write_text(html, encoding="utf-8")
    return destination.stat().st_size


def archive(resources, manifest, destination):
    signed = "signature" in manifest
    if destination.suffix != (".fabapp" if signed else ".zip"):
        raise ValueError("unsigned preparation must be .zip; signed package must be .fabapp")
    if destination.exists():
        raise ValueError("immutable package destination already exists")
    with zipfile.ZipFile(destination, "x", compression=zipfile.ZIP_STORED) as bundle:
        for name, data in [("manifest.json", canonical(manifest))] + [
                (name, (resources / name).read_bytes()) for name in sorted(manifest["files"])]:
            metadata = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            metadata.external_attr = 0o100644 << 16
            metadata.create_system = 3
            bundle.writestr(metadata, data)
    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    destination.with_suffix(destination.suffix + ".sha256").write_text(
        f"{digest}  {destination.name}\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--pkg", type=Path, required=True)
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--version", default="0.3.4")
    parser.add_argument("--platforms", default="desktop,ios,android")
    parser.add_argument("--native-dir", type=Path, help="exact-source compiled macOS runtime resources")
    parser.add_argument("--inline-html", type=Path,
                        help="also write a single self-contained HTML for Android string injection")
    args = parser.parse_args()
    platforms = [item.strip() for item in args.platforms.split(",") if item.strip()]
    manifest = prepare(args.pkg, args.resources, args.commit, args.version, platforms, args.native_dir)
    if args.inline_html is not None:
        inline_html(args.resources, args.inline_html)
    archive(args.resources, manifest, args.output)


if __name__ == "__main__":
    main()
