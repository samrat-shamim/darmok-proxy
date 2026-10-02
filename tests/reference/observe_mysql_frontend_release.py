"""Observe ordinary server-requested RELEASE acknowledgement, CLI disconnect,
and data effects. No raw-wire, authentication or proxy equivalence claim.
Expected CLI exit 1 is retained as an actual exit, never relabelled as 0.
"""
from pathlib import Path
import argparse
import json
import re
import subprocess
import traceback

from observe_mysql_transactions import Observer, require


def load(path):
    corpus = json.loads(path.read_text())
    require(set(corpus) == {"reference", "setup_sql", "reset_sql", "cases"}, "Unexpected corpus fields")
    require(set(corpus["reference"]) == {"mysql_series", "storage_engine", "autocommit", "transaction_isolation"}, "Unexpected reference fields")
    require(corpus["cases"], "No declared release cases")
    names = set()
    for case in corpus["cases"]:
        require(set(case) == {"name", "setup", "active", "completion", "closes", "chain", "ids"}, "Unexpected case fields")
        require(isinstance(case["name"], str) and case["name"] and case["name"] not in names, "Duplicate/empty case name")
        names.add(case["name"])
        require(isinstance(case["setup"], list) and all(isinstance(s, str) and s and "\n" not in s for s in case["setup"]), "Invalid setup")
        require(isinstance(case["completion"], str) and case["completion"] and "\n" not in case["completion"], "Invalid completion")
        require(all(type(case[key]) is bool for key in ["active", "closes", "chain"]), "Invalid lifecycle declaration")
        require(isinstance(case["ids"], list) and all(type(n) is int for n in case["ids"]), "Invalid expected ids")
    return corpus


class ReleaseObserver(Observer):
    def observe_case(self, number, case):
        setup = self.corpus["reset_sql"] + "\n" + "\n".join(sql + ";" for sql in case["setup"]) + "\n"
        if case["active"]:
            setup += "BEGIN;\nINSERT INTO items VALUES(1);\n"
        completion = case["completion"]
        tail = "SELECT '" + self.token + "-after';"
        sql = setup + completion + ";\n" + tail + "\n"
        command = ["docker", "exec", "-i", "-e", "MYSQL_PWD", self.args.container,
                   "mysql", "--user=" + self.args.user, "--batch", "--raw", "--skip-column-names", "--skip-reconnect", "-vvv", self.database]
        child = subprocess.run(command, input=sql, text=True, capture_output=True)
        expected_exit = 1 if case["closes"] else 0
        record = {"command": command, "sql": sql, "exit_code": child.returncode, "expected_exit_code": expected_exit,
                  "stdout": child.stdout, "stderr": child.stderr}
        path = self.run / ("lifecycle-" + str(number) + ".json")
        path.write_text(json.dumps(record, indent=2) + "\n")
        self.result["processes"].append({"category": "lifecycle", "artifact": path.name, "exit_code": child.returncode,
                                        "expected_exit_code": expected_exit})
        self.save()
        require(child.returncode == expected_exit, case["name"] + " unexpected actual CLI exit")
        heading = "--------------\n" + completion + "\n--------------\n"
        require(child.stdout.count(heading) == 1, case["name"] + " completion heading missing/duplicated")
        acknowledgement = child.stdout.split(heading)[1].split("--------------\n")[0]
        require(re.fullmatch(r"\nQuery OK, 0 rows affected \([^\n]+ sec\)\n\n", acknowledgement), case["name"] + " lacks the successful completion acknowledgement")
        line = len(sql.splitlines())
        if case["closes"]:
            require(child.stderr == f"ERROR 2013 (HY000) at line {line}: Lost connection to MySQL server during query\n", case["name"] + " does not report the exact trailing query disconnect")
            trailing = child.stdout.split("--------------\n" + tail[:-1] + "\n--------------\n")[1]
            require(trailing == "\nBye\n", case["name"] + " trailing query unexpectedly completed")
        else:
            require(child.stderr == "", case["name"] + " unexpected stderr")
            trailing = child.stdout.split("--------------\n" + tail[:-1] + "\n--------------\n")[1]
            require("1 row in set" in trailing and self.token + "-after" in trailing, case["name"] + " NO RELEASE continuation missing")
        rows, _ = self.execute("effect-" + str(number), "SELECT JSON_OBJECT('ids',(SELECT GROUP_CONCAT(n ORDER BY n) FROM items));", database=self.database)
        require(len(rows) == 1, "Expected one effect row")
        ids = [int(n) for n in json.loads(rows[0])["ids"].split(",")]
        require(ids == case["ids"], case["name"] + " completion effects differ")
        self.result["cases"].append({"name": case["name"], "completed": True, "acknowledged": True,
                                     "observed_closes": case["closes"], "actual_exit_code": child.returncode,
                                     "observed_ids": ids, "chain_wire_status": "not observed by CLI"})
        self.save()
        print(json.dumps({"name": case["name"], "result": "ordinary lifecycle observed", "actual_exit": child.returncode}), flush=True)

    def observe(self):
        created = False
        failure = False
        try:
            self.identity()
            self.absence("initial-absence")
            created = True
            rows, _ = self.execute("create-database", "CREATE DATABASE " + self.database + ";")
            require(rows == [], "Unexpected creation output")
            rows, _ = self.execute("setup", self.corpus["setup_sql"], database=self.database)
            require(rows == [], "Unexpected setup output")
            for number, case in enumerate(self.corpus["cases"], 1):
                self.observe_case(number, case)
        except Exception as error:
            failure = True
            self.result["error"] = str(error)
            self.result["traceback"] = traceback.format_exc()
        finally:
            if created:
                try:
                    rows, _ = self.execute("drop-database", "DROP DATABASE IF EXISTS " + self.database + ";")
                    require(rows == [], "Unexpected cleanup output")
                    self.absence("final-absence")
                    self.result["remaining_database_count"] = 0
                except Exception as error:
                    failure = True
                    self.result["cleanup_error"] = str(error)
            if not failure:
                try:
                    require(len(self.result["cases"]) == len(self.corpus["cases"]), "Incomplete corpus")
                    require(self.result.get("remaining_database_count") == 0, "Unconfirmed cleanup")
                    require(all(p["exit_code"] == p.get("expected_exit_code", 0) for p in self.result["processes"]), "Unexpected process failure")
                except Exception as error:
                    failure = True
                    self.result["validation_error"] = str(error)
            self.result["scope"] = __doc__
            self.result["result"] = "failed lifecycle observation" if failure else "completed stock lifecycle observations; no proxy compatibility gate"
            self.save()
        return 1 if failure else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--container", required=True)
    parser.add_argument("--image", required=True)
    parser.add_argument("--user", default="root")
    parser.add_argument("--corpus", type=Path, required=True)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    args = parser.parse_args()
    observer = ReleaseObserver(args, args.corpus, load(args.corpus))
    # Observer's existing implementation records its own helper identity. Add
    # this entry point separately; neither source is substituted for the other.
    from observe_mysql_transactions import sha256
    observer.result["entry_point_sha256"] = sha256(Path(__file__))
    (observer.run / "entry_point.py").write_bytes(Path(__file__).read_bytes())
    observer.save()
    return observer.observe()


if __name__ == "__main__":
    raise SystemExit(main())
