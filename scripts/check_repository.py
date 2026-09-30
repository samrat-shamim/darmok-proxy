#!/usr/bin/env python3
"""Check that workspace packages can be built from this repository alone."""

import json
from pathlib import Path
import subprocess
import sys


def main():
    root = Path(__file__).resolve().parent.parent
    errors = []
    if (root / "AGENTS.md").read_bytes() != (root / "CLAUDE.md").read_bytes():
        errors.append("AGENTS.md and CLAUDE.md must contain the same policy")

    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=root,
    ))
    for package in metadata["packages"]:
        manifest = Path(package["manifest_path"]).resolve()
        if not manifest.is_relative_to(root):
            errors.append(f"external workspace package: {package['name']}")
        if package["license"] != "Apache-2.0":
            errors.append(f"unexpected workspace license: {package['name']}")
        for dependency in package["dependencies"]:
            path = dependency.get("path")
            if path and not Path(path).resolve().is_relative_to(root):
                errors.append(f"external path dependency: {package['name']} -> {dependency['name']}")
            source = dependency.get("source") or ""
            if source.startswith("git+"):
                errors.append(f"unvendored Git dependency: {package['name']} -> {dependency['name']}")

    files = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
    ).decode().split("\0")
    for name in filter(None, files):
        path = root / name
        if path.is_symlink() and not path.resolve().is_relative_to(root):
            errors.append(f"symlink escapes repository: {name}")

    if errors:
        print("Repository checks failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print(f"Repository checks passed for {len(metadata['packages'])} packages")
    return 0


if __name__ == "__main__":
    sys.exit(main())
