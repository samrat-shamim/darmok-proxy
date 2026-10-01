"""Observe the declared finite transaction corpus on a stock MySQL fixture.

This is reference evidence, not a proxy or transaction compatibility gate.
The caller supplies an existing container and an explicit new evidence directory.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import traceback
import uuid


class ObservationError(Exception):
    pass


def require(condition, message):
    if not condition:
        raise ObservationError(message)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_corpus(path):
    corpus = json.loads(path.read_text())
    require(set(corpus) == {"reference", "setup_sql", "reset_sql", "cases"}, "Unexpected corpus fields")
    require(set(corpus["reference"]) == {"mysql_series", "storage_engine", "autocommit", "transaction_isolation"}, "Unexpected reference fields")
    require(isinstance(corpus["cases"], list) and corpus["cases"], "Corpus must contain cases")
    names = set()
    for case in corpus["cases"]:
        require(set(case) == {"name", "sql", "expected_errors", "effect_sql", "expected_effect"}, "Unexpected case fields")
        require(isinstance(case["name"], str) and case["name"] and case["name"] not in names, "Case names must be unique and nonempty")
        names.add(case["name"])
        for key in ["sql", "effect_sql"]:
            require(isinstance(case[key], str) and case[key].strip(), "Case SQL must be nonempty")
        require(isinstance(case["expected_errors"], list), "Expected errors must be a list")
        for error in case["expected_errors"]:
            require(set(error) == {"statement", "occurrence", "code", "sqlstate"}, "Unexpected error fields")
            require(isinstance(error["statement"], str) and error["statement"] and "\n" not in error["statement"], "Error statement must occupy one physical line")
            require(type(error["occurrence"]) is int and error["occurrence"] > 0, "Error occurrence must be positive")
            require(type(error["code"]) is int and error["code"] > 0, "Error code must be positive")
            require(isinstance(error["sqlstate"], str) and re.fullmatch(r"[A-Z0-9]{5}", error["sqlstate"]), "Expected SQLSTATE must have five characters")
    return corpus


class Observer:
    def __init__(self, args, corpus_path, corpus):
        self.args = args
        self.corpus = corpus
        self.run = args.evidence_dir
        self.run.mkdir(parents=True, exist_ok=False)
        self.token = uuid.uuid4().hex
        self.database = "darmok_reference_" + self.token
        self.result = {
            "scope": __doc__,
            "source_sha256": sha256(Path(__file__)),
            "corpus_sha256": sha256(corpus_path),
            "platform": platform.platform(),
            "python": sys.version,
            "container": args.container,
            "image": args.image,
            "database": self.database,
            "cases": [],
            "processes": [],
            "result": "in progress",
        }
        (self.run / "source.py").write_bytes(Path(__file__).read_bytes())
        (self.run / "corpus.json").write_bytes(corpus_path.read_bytes())
        self.save()

    def save(self):
        (self.run / "results.json").write_text(json.dumps(self.result, indent=2) + "\n")

    def process(self, name, category, command, sql=None, details=None):
        child = subprocess.run(command, input=sql, text=True, capture_output=True)
        record = {
            "command": command, "exit_code": child.returncode,
            "stdout": child.stdout, "stderr": child.stderr,
        }
        if sql is not None:
            record["sql"] = sql
        if details is not None:
            record.update(details)
        path = self.run / (name + ".json")
        path.write_text(json.dumps(record, indent=2) + "\n")
        self.result["processes"].append({"category": category, "artifact": path.name, "exit_code": child.returncode})
        self.save()
        require(child.returncode == 0, name + " process failed; see its receipt")
        return record

    def execute(self, name, sql, expected_errors=(), database=None):
        marker = "<DARMOK-" + self.token + "-" + name + ">"
        body = sql + "\nSELECT '" + marker + "';\n"
        command = [
            "docker", "exec", "-i", "-e", "MYSQL_PWD", self.args.container,
            "mysql", "--user=" + self.args.user, "--batch", "--raw", "--skip-column-names",
        ]
        if expected_errors:
            command.append("--force")
        if database is not None:
            command.append(database)
        record = self.process(name, "sql", command, body, {
            "expected_errors": list(expected_errors), "completion_marker": marker,
        })
        mapped = []
        for line in record["stderr"].splitlines():
            error = re.fullmatch(r"ERROR (\d+) \(([^)]+)\) at line (\d+): (.+)", line)
            require(error is not None, name + " has unexpected stderr")
            index, code, state = int(error[3]), int(error[1]), error[2]
            require(1 <= index <= len(body.splitlines()), name + " error line is outside its request")
            mapped.append((index, code, state))
        require(mapped == list(expected_errors), name + " error attribution differs from its exact statement/occurrence")
        lines = record["stdout"].splitlines()
        require(lines and lines[-1] == marker and lines.count(marker) == 1, name + " lacks its exact final completion marker")
        record["mapped_errors"] = [
            {"line": index, "code": code, "sqlstate": state, "statement": body.splitlines()[index - 1]}
            for index, code, state in mapped
        ]
        (self.run / (name + ".json")).write_text(json.dumps(record, indent=2) + "\n")
        return lines[:-1], record

    def identity(self):
        root = Path(__file__).resolve().parents[2]
        head = self.process("git-head", "metadata", ["git", "-C", str(root), "rev-parse", "HEAD"])
        status = self.process("git-status", "metadata", ["git", "-C", str(root), "status", "--porcelain"])
        version = self.process("docker-version", "metadata", ["docker", "--version"])
        self.result["git_head"] = head["stdout"].strip()
        self.result["git_worktree_dirty"] = bool(status["stdout"])
        self.result["docker_version"] = version["stdout"].strip()
        formats = [
            ("container", self.args.container, '{"id":{{json .Id}},"image":{{json .Image}}}'),
            ("image", self.args.image, '{"id":{{json .Id}},"repo_digests":{{json .RepoDigests}},"os":{{json .Os}},"architecture":{{json .Architecture}}}'),
        ]
        identities = {}
        for label, target, template in formats:
            record = self.process(label + "-inspection", "inspection", ["docker", "inspect", "--format", template, target])
            require(record["stderr"] == "", label + " inspection has unexpected stderr")
            identities[label] = json.loads(record["stdout"])
        require(identities["container"]["image"] == identities["image"]["id"], "Fixture container does not use the selected image")
        self.result["identities"] = identities
        metadata, _ = self.execute("server", "SELECT JSON_OBJECT('version',VERSION(),'sql_mode',@@session.sql_mode,'autocommit',@@session.autocommit,'isolation',@@session.transaction_isolation,'storage_engine',@@default_storage_engine);")
        require(len(metadata) == 1, "Server metadata must have exactly one row")
        server = json.loads(metadata[0])
        reference = self.corpus["reference"]
        require(server["version"].startswith(reference["mysql_series"] + "."), "Fixture MySQL series differs from the reference")
        require(server["storage_engine"] == reference["storage_engine"] and server["autocommit"] == reference["autocommit"] and server["isolation"] == reference["transaction_isolation"], "Fixture engine/session settings differ from the reference")
        self.result["server"] = server
        self.save()

    def absence(self, name):
        rows, _ = self.execute(name, "SELECT COUNT(*) FROM information_schema.schemata WHERE schema_name='" + self.database + "';")
        require(rows == ["0"], "The assigned disposable database is not absent")

    def observe_case(self, number, case):
        body = self.corpus["reset_sql"] + "\n" + case["sql"] + "\n" + case["effect_sql"] + "\n"
        expected = []
        for error in case["expected_errors"]:
            indices = [index for index, line in enumerate(body.splitlines(), 1) if line.strip() == error["statement"]]
            require(len(indices) >= error["occurrence"], case["name"] + " lacks the declared erroneous occurrence")
            expected.append((indices[error["occurrence"] - 1], error["code"], error["sqlstate"]))
        require(expected == sorted(expected), case["name"] + " errors must follow statement order")
        lines, record = self.execute("case-" + str(number), body, expected, self.database)
        require(len(lines) == 1, case["name"] + " must return exactly one effect row")
        actual = json.loads(lines[0])
        # Compare JSON representations so booleans cannot equal integer counts.
        canonical = lambda value: json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False)
        require(canonical(actual) == canonical(case["expected_effect"]), case["name"] + " effects differ")
        self.result["cases"].append({
            "name": case["name"], "expected_effect": case["expected_effect"], "observed_effect": actual,
            "mapped_errors": record["mapped_errors"], "completed": True,
        })
        self.save()
        print(json.dumps({"name": case["name"], "result": "ordinary reference observed", "effect": actual}), flush=True)

    def observe(self):
        creation_attempted = False
        failed = False
        try:
            self.identity()
            self.absence("initial-absence")
            # The fresh assigned name is ours even if CREATE's receipt is lost.
            creation_attempted = True
            rows, _ = self.execute("create-database", "CREATE DATABASE " + self.database + ";")
            require(rows == [], "CREATE DATABASE has unexpected output")
            rows, _ = self.execute("setup", self.corpus["setup_sql"], database=self.database)
            require(rows == [], "Fixture setup has unexpected output")
            for number, case in enumerate(self.corpus["cases"], 1):
                self.observe_case(number, case)
        except Exception as error:
            failed = True
            self.result["error"] = str(error)
            self.result["traceback"] = traceback.format_exc()
        finally:
            if creation_attempted:
                try:
                    rows, _ = self.execute("drop-database", "DROP DATABASE IF EXISTS " + self.database + ";")
                    require(rows == [], "DROP DATABASE has unexpected output")
                    self.absence("final-absence")
                    self.result["remaining_database_count"] = 0
                except Exception as error:
                    failed = True
                    self.result["cleanup_error"] = str(error)
                    self.result["cleanup_traceback"] = traceback.format_exc()
            if not failed:
                try:
                    require(len(self.result["cases"]) == len(self.corpus["cases"]), "Not every declared case completed")
                    require(self.result.get("remaining_database_count") == 0, "Database cleanup is unconfirmed")
                    require(all(p["exit_code"] == 0 for p in self.result["processes"]), "A fixture process failed")
                except Exception as error:
                    failed = True
                    self.result["validation_error"] = str(error)
            self.result["result"] = "failed reference observation" if failed else "completed stock reference observations; no proxy compatibility gate"
            self.result["artifact_sha256"] = {
                p.name: sha256(p) for p in self.run.iterdir() if p.is_file() and p.name != "results.json"
            }
            self.save()
            counts = {category: sum(p["category"] == category for p in self.result["processes"]) for category in ["sql", "inspection", "metadata"]}
            print(json.dumps({
                "result": self.result["result"], "cases": len(self.result["cases"]), "process_counts": counts,
                "remaining_database_count": self.result.get("remaining_database_count"),
                "source_sha256": self.result["source_sha256"], "corpus_sha256": self.result["corpus_sha256"],
                "git_head": self.result.get("git_head"), "git_worktree_dirty": self.result.get("git_worktree_dirty"),
                "server": self.result.get("server"), "identities": self.result.get("identities"),
                "error": self.result.get("error"), "cleanup_error": self.result.get("cleanup_error"),
            }), flush=True)
        return 1 if failed else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--container", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--user", default="root")
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--corpus", type=Path, required=True)
    args = parser.parse_args()
    require("MYSQL_PWD" in os.environ, "Set MYSQL_PWD for the stock fixture")
    corpus_path = args.corpus
    corpus = load_corpus(corpus_path)
    return Observer(args, corpus_path, corpus).observe()


if __name__ == "__main__":
    sys.exit(main())
