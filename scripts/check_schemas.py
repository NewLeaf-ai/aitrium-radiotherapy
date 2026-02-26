#!/usr/bin/env python3
from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCHEMAS = [
    ROOT / "schemas" / "error.schema.json",
    ROOT / "schemas" / "rt_inspect.input.schema.json",
    ROOT / "schemas" / "rt_inspect.output.schema.json",
    ROOT / "schemas" / "rt_dvh.input.schema.json",
    ROOT / "schemas" / "rt_dvh.output.schema.json",
    ROOT / "schemas" / "rt_dvh_metrics.input.schema.json",
    ROOT / "schemas" / "rt_dvh_metrics.output.schema.json",
]


def main() -> int:
    for schema in SCHEMAS:
        if not schema.exists():
            print(f"Missing schema: {schema}")
            return 1
        try:
            parsed = json.loads(schema.read_text())
        except json.JSONDecodeError as error:
            print(f"Invalid JSON in {schema}: {error}")
            return 1

        if "$schema" not in parsed:
            print(f"Schema missing $schema declaration: {schema}")
            return 1

    print("Schema validation check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
