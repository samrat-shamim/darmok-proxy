#!/usr/bin/env python3
"""Pinned stock exact-number columns, text rows and warnings; no proxy exchange."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from mysql_cli_columns import columns_from_cli


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--container", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument("--evidence-dir", required=True, type=Path)
    args = parser.parse_args()
    args.evidence_dir.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[2]
    result = {
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "source_status": subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True),
        "observer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "column_decoder_sha256": hashlib.sha256((Path(__file__).parent / "mysql_cli_columns.py").read_bytes()).hexdigest(),
        "corpus_sha256": hashlib.sha256(args.corpus.read_bytes()).hexdigest(),
        "commands": [], "cases": [], "complete": False,
    }

    def run(name, command):
        child = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for suffix, stream in [("stdout", child.stdout), ("stderr", child.stderr)]:
            with (args.evidence_dir / (name + "." + suffix)).open("xb") as output:
                output.write(stream)
        result["commands"].append({"name": name, "command": command, "exit_code": child.returncode,
                                   "stdout_sha256": hashlib.sha256(child.stdout).hexdigest(),
                                   "stderr_sha256": hashlib.sha256(child.stderr).hexdigest()})
        if child.returncode != 0 or child.stderr:
            raise RuntimeError(name + " did not complete cleanly")
        return child.stdout.decode("utf-8")

    try:
        container = json.loads(run("container", ["docker", "inspect", "--format", '{"id":{{json .Id}},"image":{{json .Image}}}', args.container]))
        image = json.loads(run("image", ["docker", "image", "inspect", "--format", '{"id":{{json .Id}},"digests":{{json .RepoDigests}}}', args.image]))
        if container["image"] != image["id"]:
            raise RuntimeError("container differs from the pinned reference image")
        client = ["docker", "exec", "-i", "-e", "MYSQL_PWD", container["id"],
                  "mysql", "--user=root", "--default-character-set=utf8mb4"]
        version = run("version", client + ["--batch", "--skip-column-names", "--execute", "SELECT VERSION();"]).strip()
        corpus = json.loads(args.corpus.read_text())
        if version != corpus["server_version"]:
            raise RuntimeError("stock server differs from the observed version")
        result.update(container=container, image=image, version=version)
        for index, case in enumerate(corpus["cases"]):
            sql = "SET NAMES utf8mb4 COLLATE utf8mb4_general_ci; " + case["sql"]
            output = run("case-" + str(index) + "-columns", client + ["--column-type-info", "--verbose", "--verbose", "--verbose", "--execute", sql])
            columns = columns_from_cli(output, case["name"])
            if columns != case["columns"]:
                raise RuntimeError(case["name"] + " columns differ: " + json.dumps(columns))
            text = run("case-" + str(index) + "-row", client + ["--batch", "--raw", "--skip-column-names", "--execute", sql + " SELECT @@warning_count;"])
            lines = text.splitlines()
            if len(lines) != 2 or lines[0].split("\t") != case["row"] or lines[1] != str(case["warning_count"]):
                raise RuntimeError(case["name"] + " exact row or warnings differ: " + repr(text))
            if len(case["row"]) != len(columns):
                raise RuntimeError(case["name"] + " row width differs from its declaration")
            result["cases"].append({"name": case["name"], "columns": columns,
                                    "row": lines[0].split("\t"), "warning_count": int(lines[1]), "completed": True})
        result["complete"] = True
    finally:
        with (args.evidence_dir / "observations.json").open("x") as output:
            json.dump(result, output, indent=2)
            output.write("\n")
    print(json.dumps({"complete": result["complete"], "cases": len(result["cases"])}))


if __name__ == "__main__":
    main()
