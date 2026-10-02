#!/usr/bin/env python3
"""Pinned stock substring values and warnings; not frontend proxy admission."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def expression(case):
    value = case["input"]
    if value is None:
        source = "NULL"
    elif case["kind"] == "utf8":
        source = "CONVERT(UNHEX('" + value.encode("utf-8").hex() + "') USING utf8mb4)"
    elif case["kind"] == "bytes":
        source = "UNHEX('" + bytes.fromhex(value).hex() + "')"
    else:
        raise ValueError("unknown corpus kind")

    def integer(value):
        if value is None:
            return "NULL"
        if type(value) is not int or not -(2**63) <= value < 2**63:
            raise ValueError("corpus argument is not a signed 64-bit integer")
        return str(value)

    args = [source, integer(case["start"])]
    if case["arity"] == 3:
        args.append(integer(case["length"]))
    elif case["arity"] != 2:
        raise ValueError("unknown corpus arity")
    return "SUBSTRING(" + ", ".join(args) + ")"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--container", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument("--evidence-dir", required=True, type=Path)
    args = parser.parse_args()
    args.evidence_dir.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[2]
    corpus = json.loads(args.corpus.read_text())
    result = {
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
        "source_tree": subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=root, text=True).strip(),
        "source_status": subprocess.check_output(["git", "status", "--porcelain"], cwd=root, text=True),
        "observer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "corpus_sha256": hashlib.sha256(args.corpus.read_bytes()).hexdigest(),
        "commands": [], "completed_cases": 0, "complete": False,
    }

    def run(name, command, request=None):
        payload = request.encode("utf-8") if request is not None else None
        if payload is not None:
            with (args.evidence_dir / (name + ".sql")).open("xb") as stream:
                stream.write(payload)
        child = subprocess.run(command, input=payload, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for suffix, data in [("stdout", child.stdout), ("stderr", child.stderr)]:
            with (args.evidence_dir / (name + "." + suffix)).open("xb") as stream:
                stream.write(data)
        result["commands"].append({
            "name": name, "command": command, "exit_code": child.returncode,
            "request_sha256": hashlib.sha256(payload).hexdigest() if payload is not None else None,
            "stdout_sha256": hashlib.sha256(child.stdout).hexdigest(),
            "stderr_sha256": hashlib.sha256(child.stderr).hexdigest(),
        })
        if child.returncode != 0 or child.stderr:
            raise RuntimeError(name + " did not complete cleanly")
        return child.stdout.decode("utf-8")

    client = ["docker", "exec", "-i", "-e", "MYSQL_PWD", args.container,
              "mysql", "--user=root", "--default-character-set=utf8mb4", "--batch", "--raw", "--skip-column-names"]
    try:
        container = json.loads(run("container", ["docker", "inspect", "--format", '{"id":{{json .Id}},"image":{{json .Image}}}', args.container]))
        image = json.loads(run("image", ["docker", "image", "inspect", "--format", '{"id":{{json .Id}},"digests":{{json .RepoDigests}}}', args.image]))
        if container["image"] != image["id"] or args.image not in image["digests"]:
            raise RuntimeError("container differs from the pinned reference image")
        version = run("version", client, "SELECT VERSION();\n").strip()
        if version != corpus["reference"]["version"] or not version.startswith("8.4."):
            raise RuntimeError("stock server version differs from the corpus")
        result.update(container=container, image=image, server_version=version)
        request = "SET NAMES utf8mb4 COLLATE utf8mb4_0900_ai_ci;\n"
        expected = []
        for index, case in enumerate(corpus["cases"]):
            request += f"SELECT {index}, IF(r IS NULL, 'NULL', HEX(r)) FROM (SELECT {expression(case)} AS r) AS observed;\n"
            request += f"SELECT 'warnings', {index}, @@warning_count;\n"
            value = case["expected"]
            encoded = "NULL" if value is None else (value.encode("utf-8").hex() if case["kind"] == "utf8" else value).upper()
            if case["warnings"] != 0:
                raise ValueError("typed corpus unexpectedly expects warnings")
            expected.extend([f"{index}\t{encoded}", f"warnings\t{index}\t0"])
        observed = run("substrings", client, request).splitlines()
        if observed != expected:
            index = next((i for i, (actual, wanted) in enumerate(zip(observed, expected)) if actual != wanted), min(len(observed), len(expected)))
            raise RuntimeError("stock substring value or warnings differ at output row " + str(index))
        result.update(completed_cases=len(corpus["cases"]), complete=True)
    finally:
        with (args.evidence_dir / "observations.json").open("x") as stream:
            json.dump(result, stream, indent=2)
            stream.write("\n")
    print(json.dumps({"complete": result["complete"], "cases": result["completed_cases"]}))


if __name__ == "__main__":
    main()
