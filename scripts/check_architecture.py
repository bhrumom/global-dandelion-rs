#!/usr/bin/env python3
"""Static boundary gate, not a substitute for behavioral tests or review."""
import json
import re
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {
    "dandelion-hotspot": {"dandelion-core"},
    "dandelion-updater": {"dandelion-core"},
    "dandelion-state": {"dandelion-core"},
    "dandelion-tui": {"dandelion-core"},
    "dandelion-core": set(),
    "dandelion-config": {"dandelion-core"},
    "dandelion-packet": {"dandelion-core"},
    "dandelion-pacing": {"dandelion-core"},
    "dandelion-platform-linux": {"dandelion-core"},
    "dandelion-scanner": {"dandelion-core"},
    "dandelion-transport": {"dandelion-core", "dandelion-packet"},
    "dandelion-app": {"dandelion-core", "dandelion-config", "dandelion-packet", "dandelion-pacing",
                      "dandelion-platform-linux", "dandelion-scanner", "dandelion-transport", "dandelion-state", "dandelion-tui", "dandelion-updater", "dandelion-hotspot"},
}
found = set()
for manifest in sorted((ROOT / "crates").glob("*/Cargo.toml")):
    package = tomllib.loads(manifest.read_text())
    name = package["package"]["name"]
    assert name in ALLOWED, f"undeclared architecture component: {name}"
    assert package["lints"]["workspace"], f"{name}: workspace lint policy required"
    assert package["package"]["publish"]["workspace"], f"{name}: publishing must be explicit"
    found.add(name)
    internal = {key for key in package.get("dependencies", {}) if key.startswith("dandelion-")}
    assert internal <= ALLOWED[name], f"{name}: forbidden dependencies {internal - ALLOWED[name]}"
    if name == "dandelion-core":
        assert not set(package["dependencies"]) & {"tokio", "rustix", "walkdir", "clap", "tracing"}
    for source in manifest.parent.rglob("*.rs"):
        if "tests" in source.parts or "benches" in source.parts:
            continue
        production = source.read_text().split("#[cfg(test)]")[0]
        for forbidden in [r"\bunsafe\s*\{", r"\bunbounded_channel\s*\(",
                          r"\bprocess::exit\s*\(", r"\bstatic\s+mut\b",
                          r"\bthread::spawn\s*\(", r"\btokio::spawn\s*\("]:
            assert not re.search(forbidden, production), f"{source}: prohibited {forbidden}"
assert found == set(ALLOWED), f"missing packages: {set(ALLOWED) - found}"
print(json.dumps({"architecture": "pass", "components": sorted(found)}))
