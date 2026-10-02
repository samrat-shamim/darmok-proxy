"""Decode declared CLI fields; NUM is computed by the client, not a wire flag."""
import re


def columns_from_cli(output, case_name):
    fields = re.findall(
        r"(?ms)^Field\s+(\d+):\s+`(.*?)`\nCatalog:\s+`(.*?)`\nDatabase:\s+`(.*?)`\n"
        r"Table:\s+`(.*?)`\nOrg_table:\s+`(.*?)`\nType:\s+(\w+)\n"
        r"Collation:\s+[^\n]*\((\d+)\)\nLength:\s+(\d+)\nMax_length:\s+\d+\n"
        r"Decimals:\s+(\d+)\nFlags: *([^\n]*)", output)
    columns = []
    for ordinal, name, catalog, database, table, original_table, kind, charset, width, decimals, flags in fields:
        if int(ordinal) != len(columns) + 1 or (catalog, database, table, original_table) != ("def", "", "", ""):
            raise RuntimeError(case_name + " has unexpected source identity")
        columns.append({"name": name, "type": kind, "charset": int(charset), "width": int(width),
                        "decimals": int(decimals), "cli_flags": flags.split()})
    return columns
