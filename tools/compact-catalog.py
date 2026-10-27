#!/usr/bin/env python3
"""Project a Moodle catalog dump onto the metadata used by student-mcp.

Usage: python tools/compact-catalog.py [dump.json] [--output src/services/moodle/catalog/functions.json]
The checked-in catalog can also be its own input; generation is idempotent.
Input parameter metadata is preserved verbatim (including optional/nullable).
"""
import argparse
import json
from pathlib import Path


def compact(source):
    functions = []
    names = set()
    for entry in source["functions"]:
        name = entry["name"]
        if name in names or entry["type"] not in ("read", "write"):
            raise ValueError(f"Duplicate function or invalid access classification: {name}")
        names.add(name)
        functions.append({key: entry[key] for key in ("name", "type", "description", "parameters")})
    return {"generator": "tools/compact-catalog.py", "moodle": source["moodle"], "functions": functions}


def encode(catalog):
    # One function per line keeps diffs readable without shipping indentation.
    dump = lambda value: json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    header = {key: value for key, value in catalog.items() if key != "functions"}
    return dump(header)[:-1] + ',"functions":[\n' + ',\n'.join(map(dump, catalog["functions"])) + '\n]}\n'


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, nargs="?", default=Path("src/services/moodle/catalog/functions.json"))
    parser.add_argument("--output", type=Path, default=Path("src/services/moodle/catalog/functions.json"))
    args = parser.parse_args()
    source = json.loads(args.source.read_text(encoding="utf-8"))
    result = encode(compact(source))
    args.output.write_text(result, encoding="utf-8")
    print(f"{len(source['functions'])} functions; {len(result.encode('utf-8')):,} bytes")
