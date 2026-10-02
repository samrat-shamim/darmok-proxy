#!/usr/bin/env python3
"""Pinned stock CLI column observations; not a proxy or raw-wire differential."""
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
    records = []
    result = {"source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
              "source_status": subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True),
              "observer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "column_decoder_sha256": hashlib.sha256((Path(__file__).parent / "mysql_cli_columns.py").read_bytes()).hexdigest(),
              "corpus_sha256": hashlib.sha256(args.corpus.read_bytes()).hexdigest(),
              "commands": records, "complete": False}

    def run(name, command):
        child = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for suffix, stream in [("stdout", child.stdout), ("stderr", child.stderr)]:
            with (args.evidence_dir / (name + "." + suffix)).open("xb") as output:
                output.write(stream)
        records.append({"name": name, "command": command, "exit_code": child.returncode,
                        "stdout_sha256": hashlib.sha256(child.stdout).hexdigest(),
                        "stderr_sha256": hashlib.sha256(child.stderr).hexdigest()})
        if child.returncode != 0 or child.stderr:
            raise RuntimeError(name + " did not complete cleanly")
        return child.stdout.decode("utf-8")

    client = ["docker", "exec", "-i", "-e", "MYSQL_PWD", args.container,
              "mysql", "--user=root", "--default-character-set=utf8mb4"]
    try:
        container = json.loads(run("container", ["docker", "inspect", "--format", '{"id":{{json .Id}},"image":{{json .Image}}}', args.container]))
        image = json.loads(run("image", ["docker", "image", "inspect", "--format", '{"id":{{json .Id}},"digests":{{json .RepoDigests}}}', args.image]))
        if container["image"] != image["id"]:
            raise RuntimeError("container differs from the pinned reference image")
        version = run("version", client + ["--batch", "--skip-column-names", "--execute", "SELECT VERSION();"]).strip()
        corpus = json.loads(args.corpus.read_text())
        if version != corpus["server_version"]:
            raise RuntimeError("stock server differs from the observed version")
        result.update(container=container, image=image, version=version, cases=[])
        for index, case in enumerate(corpus["cases"]):
            output = run("case-" + str(index), client + ["--column-type-info", "--verbose", "--verbose", "--verbose", "--execute", "SET NAMES utf8mb4 COLLATE utf8mb4_general_ci; " + case["sql"]])
            columns = columns_from_cli(output, case["name"])
            if columns != case["columns"]:
                raise RuntimeError(case["name"] + " columns differ: " + json.dumps(columns, ensure_ascii=False))
            result["cases"].append({"name": case["name"], "columns": columns, "completed": True})
        result["complete"] = True
    finally:
        with (args.evidence_dir / "observations.json").open("x") as output:
            json.dump(result, output, indent=2, ensure_ascii=False)
            output.write("\n")
    print(json.dumps({"complete": result["complete"], "cases": len(result["cases"])}))


if __name__ == "__main__":
    main()
