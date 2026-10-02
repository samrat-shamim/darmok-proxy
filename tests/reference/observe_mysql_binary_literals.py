#!/usr/bin/env python3
"""Pinned stock client fields and bytes; no proxy handshake or auth implementation."""
import argparse
import hashlib
import importlib.metadata
import json
import os
from pathlib import Path
import platform
import subprocess

import pymysql


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--container", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument("--evidence-dir", required=True, type=Path)
    args = parser.parse_args()
    args.evidence_dir.mkdir(parents=True, exist_ok=False)
    root = Path(__file__).resolve().parents[2]
    records = []
    result = {"complete": False, "cases": [], "commands": records,
              "observer_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              "corpus_sha256": hashlib.sha256(args.corpus.read_bytes()).hexdigest(),
              "python": platform.python_version(),
              "pymysql_distribution": importlib.metadata.version("pymysql"),
              "pymysql_module": pymysql.__version__}
    connection = None

    def run(name, command):
        child = subprocess.run(command, cwd=root, capture_output=True)
        for suffix, data in (("stdout", child.stdout), ("stderr", child.stderr)):
            with (args.evidence_dir / (name + "." + suffix)).open("xb") as stream:
                stream.write(data)
        records.append({"name": name, "command": command, "actual_exit_code": child.returncode,
                        "stdout_sha256": hashlib.sha256(child.stdout).hexdigest(),
                        "stderr_sha256": hashlib.sha256(child.stderr).hexdigest()})
        if child.returncode != 0 or child.stderr:
            raise RuntimeError(name + " failed")
        return child.stdout.decode("utf8").strip()

    def observe(cursor, sql):
        try:
            cursor.execute(sql)
            fields = cursor._result.fields
            warning_count = cursor._result.warning_count
            rows = cursor.fetchall()
            return {"kind": "result", "warning_count": warning_count,
                    "columns": [{"name_hex": f.name.encode("utf8").hex(),
                                 "org_name_hex": f.org_name.encode("utf8").hex(),
                                 "schema_hex": f.db.hex(),
                                 "table_hex": f.table_name.encode("utf8").hex(),
                                 "org_table_hex": f.org_table.encode("utf8").hex(),
                                 "type": f.type_code, "charset": f.charsetnr,
                                 "width": f.length, "flags": f.flags, "decimals": f.scale}
                                for f in fields],
                    "rows_hex": [[None if value is None else
                                  (value.hex() if isinstance(value, bytes) else
                                   str(value).encode("utf8").hex()) for value in row] for row in rows]}
        except pymysql.MySQLError as error:
            return {"kind": "error", "error_code": error.args[0], "message": error.args[1]}

    try:
        if result["pymysql_distribution"] != "1.1.2":
            raise RuntimeError("the pinned PyMySQL distribution is required")
        result["source_revision"] = run("revision", ["git", "rev-parse", "HEAD"])
        result["source_tree"] = run("tree", ["git", "rev-parse", "HEAD^{tree}"])
        result["source_status"] = run("status", ["git", "status", "--porcelain"])
        container = json.loads(run("container", ["docker", "inspect", "--format",
                              '{"image":{{json .Image}}}', args.container]))
        image = json.loads(run("image", ["docker", "image", "inspect", "--format",
                          '{"id":{{json .Id}},"digests":{{json .RepoDigests}}}', args.image]))
        digest = args.image.rsplit("@", 1)[-1]
        if not digest.startswith("sha256:") or container["image"] != image["id"] or not any(
                name.rsplit("@", 1)[-1] == digest for name in image["digests"]):
            raise RuntimeError("the stock fixture differs from the pinned image")
        result.update(container=container, image=image)
        fixture_uuid = run("server-uuid", ["docker", "exec", "-i", "-e", "MYSQL_PWD",
                           args.container, "mysql", "--user=root", "--batch", "--skip-column-names",
                           "--execute", "SELECT @@server_uuid"])
        corpus = json.loads(args.corpus.read_text())
        connection = pymysql.connect(host=args.host, port=args.port, user="root",
                                     password=os.environ["MYSQL_PWD"], charset="utf8mb4",
                                     autocommit=True)
        with connection.cursor() as cursor:
            cursor.execute("SELECT VERSION(), @@server_uuid")
            result["server_version"], result["server_uuid"] = cursor.fetchone()
            if result["server_version"] != corpus["server_version"] or result["server_uuid"] != fixture_uuid:
                raise RuntimeError("connected stock instance differs")
            cursor.execute("SET NAMES utf8mb4 COLLATE utf8mb4_general_ci")
            for group in ("cases", "syntax_errors", "unsupported"):
                for index, case in enumerate(corpus[group]):
                    cursor.execute("SET SESSION sql_mode=%s", (case["sql_mode"],))
                    actual = observe(cursor, case["sql"])
                    expected = {k: v for k, v in case.items() if k not in ("name", "sql_mode", "sql")}
                    record = {"group": group, "index": index, "name": case["name"],
                              "sql_mode": case["sql_mode"], "sql": case["sql"], "observed": actual,
                              "matches": actual == expected}
                    with (args.evidence_dir / (group + "-" + str(index) + ".json")).open("x") as output:
                        json.dump(record, output, indent=2)
                        output.write("\n")
                    result["cases"].append(record)
                    if not record["matches"]:
                        raise RuntimeError(group + " case differs: " + case["name"])
        result["complete"] = True
    finally:
        if connection is not None:
            connection.close()
        with (args.evidence_dir / "observations.json").open("x") as output:
            json.dump(result, output, indent=2)
            output.write("\n")
    print(json.dumps({"complete": result["complete"], "cases": len(result["cases"])}))


if __name__ == "__main__":
    main()
