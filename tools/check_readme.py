"""Cross-check the README against the code, so the two cannot drift silently.

Run with `python tools/check_readme.py`. Exits non-zero on a mismatch, which is
what makes it worth wiring into CI.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates" / "pumpkin-papi" / "src"
README = ROOT / "README.md"

failures: list[str] = []


def fail(label: str, expected: object, found: object) -> None:
    print(f"FAIL {label}\n       code:   {expected}\n       README: {found}")
    failures.append(label)


def ok(label: str, value: object) -> None:
    print(f"ok   {label}: {value}")


builtins_src = (SRC / "builtins.rs").read_text(encoding="utf-8")
protocol_src = (SRC / "protocol.rs").read_text(encoding="utf-8")
readme = README.read_text(encoding="utf-8")

# --- built in placeholders -------------------------------------------------

table = builtins_src.split("pub static BUILTINS")[1].split("\n];")[0]
code_ids = sorted(set(re.findall(r'id:\s*"([a-z_]+)"', table)))

# Only the table itself, not the prose below it that lists absent placeholders.
documented: set[str] = set()
in_table = False
for line in readme.splitlines():
    if line.startswith("## Built in placeholders"):
        in_table = True
    elif in_table and line.startswith("## "):
        in_table = False
    elif in_table:
        # Matches both `%player_name%` and `%player_has_permission:NODE%`.
        documented.update(re.findall(r"^\| `%([a-z_]+)(?::[A-Z]+)?%`", line))

if code_ids == sorted(documented):
    ok("built in placeholders", f"{len(code_ids)} documented and correct")
else:
    undocumented = sorted(set(code_ids) - documented)
    invented = sorted(documented - set(code_ids))
    fail(
        "built in placeholders",
        f"{len(code_ids)} in code; missing from README: {undocumented or 'none'}; "
        f"in README but not code: {invented or 'none'}",
        f"{len(documented)} documented",
    )

# The count in the log line an admin is told to look for.
claimed = re.search(r"ready with (\d+) built in placeholders", readme)
if claimed and int(claimed.group(1)) != len(code_ids):
    fail("log line count", len(code_ids), f"{claimed.group(1)} in README")
elif claimed:
    ok("log line count", len(code_ids))

# --- protocol operations ---------------------------------------------------

request_block = protocol_src.split("pub enum Request")[1].split("\n}")[0]
variants = re.findall(r"\n    (\w+)", request_block)
# `#[serde(rename_all = "snake_case")]` is what puts these on the wire, and the
# README documents wire names, so convert before looking for a section.
ops = {re.sub(r"(?<!^)(?=[A-Z])", "_", variant).lower() for variant in variants}
for op in sorted(ops):
    if f"### `{op}`" in readme:
        ok(f"op {op}", "documented")
    else:
        fail(f"op {op}", f"a `### `{op}` section", "not documented")

# --- reserved namespaces ---------------------------------------------------

reserved = re.search(r"RESERVED_NAMESPACES: \[&str; \d+\] = \[([^\]]+)\]", protocol_src)
code_namespaces = (
    sorted(x.strip().strip('"') for x in reserved.group(1).split(",")) if reserved else []
)
if all(name in readme for name in code_namespaces):
    ok("reserved namespaces", ", ".join(code_namespaces))
else:
    fail("reserved namespaces", ", ".join(code_namespaces), "at least one undocumented")

print()
if failures:
    print(f"{len(failures)} mismatch(es). The README has drifted from the code.")
    sys.exit(1)
print("README matches the code.")
