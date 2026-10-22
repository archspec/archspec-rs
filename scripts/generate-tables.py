#!/usr/bin/env python3
"""Regenerate checked-in Rust tables from the archspec-json submodule.

Strings are deduplicated in a NUL-delimited pool. Records refer to the pool and
other tables by integer offsets so the compiled data needs no pointer fixups.
The runtime record layouts are documented beside their generated tables.
"""

import argparse
from graphlib import TopologicalSorter
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
MISSING = (1 << 16) - 1


def integer(value, maximum=MISSING):
    if not isinstance(value, int) or not 0 <= value <= maximum:
        raise ValueError(f"integer outside 0..={maximum}: {value!r}")
    return value


def optional_integer(value):
    return MISSING if value is None else integer(value, MISSING - 1)


def words(values):
    if isinstance(values, str):
        values = [values]
    joined = " ".join(values or ())
    if joined.split() != list(values or ()):
        raise ValueError(f"word list cannot be encoded: {values!r}")
    return joined


def rust_string(value):
    result = []
    for index, char in enumerate(value):
        if char in ("\\", '"'):
            result.append("\\" + char)
        elif not char.isprintable() or (index == 0 and char.isspace()):
            # A Rust string continuation discards leading whitespace.
            result.append(f"\\u{{{ord(char):x}}}")
        else:
            result.append(char)
    return "".join(result)


class StringPool:
    def __init__(self):
        self.offsets = {}
        self.size = 0

    def add(self, value):
        if value is None:
            return MISSING
        if "\0" in value:
            raise ValueError(f"string contains NUL: {value!r}")
        if value not in self.offsets:
            self.offsets[value] = integer(self.size, MISSING - 1)
            self.size += len(value.encode("utf-8")) + 1
            integer(self.size, MISSING)
        return self.offsets[value]

    def render(self):
        entries = [rust_string(value) + "\\0" for value in self.offsets]
        return 'pub(super) const STRINGS: &str = "' + "\\\n    ".join(entries) + '";'


def table(name, rows, width, kind="u16"):
    for row in rows:
        if len(row) != width:
            raise ValueError(f"invalid {name} row: {row!r}")
        for value in row:
            integer(value, (1 << 32) - 1 if kind == "u32" else MISSING)
    values = "\n".join("    [" + ", ".join(map(str, row)) + "]," for row in rows)
    return f"pub(super) const {name}: &[[{kind}; {width}]] = &[\n{values}\n];"


def architectures(data, pool):
    targets, compilers, versions = [], [], []
    catalog = data["microarchitectures"]
    # The runtime constructs the target graph in a single pass, parents first.
    parents = {name: words(catalog[name]["from"]).split() for name in sorted(catalog)}
    for name in TopologicalSorter(parents).static_order():
        target = catalog[name]
        compiler_start, compiler_end = MISSING, 0
        if target.get("compilers") is not None:
            compiler_start = integer(len(compilers), MISSING - 1)
            for compiler, entries in sorted(target["compilers"].items()):
                single = isinstance(entries, dict)
                version_start = integer(len(versions))
                for entry in [entries] if single else entries:
                    versions.append(
                        [
                            pool.add(entry["versions"]),
                            pool.add(entry["flags"]),
                            pool.add(entry.get("name")),
                        ]
                    )
                compilers.append(
                    [
                        pool.add(compiler),
                        int(single),
                        version_start,
                        integer(len(versions)),
                    ]
                )
            compiler_end = integer(len(compilers))
        targets.append(
            [
                pool.add(name),
                pool.add(" ".join(parents[name])),
                pool.add(target["vendor"]),
                pool.add(words(target["features"])),
                optional_integer(target.get("generation")),
                pool.add(target.get("cpupart")),
                compiler_start,
                compiler_end,
            ]
        )

    aliases = [
        [
            pool.add(name),
            pool.add(alias.get("reason")),
            pool.add(words(alias["any_of"]))
            if alias.get("any_of") is not None
            else MISSING,
            pool.add(words(alias["families"]))
            if alias.get("families") is not None
            else MISSING,
        ]
        for name, alias in sorted(data["feature_aliases"].items())
    ]
    conversions = data["conversions"]
    description = pool.add(conversions["description"])
    mappings = {
        key: [
            [pool.add(name), pool.add(value)]
            for name, value in sorted(conversions[key].items())
        ]
        for key in ("arm_vendors", "darwin_flags")
    }
    return "\n".join(
        [
            "// String fields are byte offsets in STRINGS. u16::MAX denotes a missing value.",
            "// Table ranges are end-exclusive. Every target appears after its parents.",
            "// [name, parents, vendor, features, generation, cpupart, compiler_start, compiler_end]",
            table("TARGETS", targets, 8),
            "// [name, single, version_start, version_end]",
            table("COMPILERS", compilers, 4),
            "// [versions, flags, name]",
            table("VERSIONS", versions, 3),
            "// [name, reason, any_of, families]",
            table("ALIASES", aliases, 4),
            f"pub(super) const DESCRIPTION: u16 = {description};",
            *[table(key.upper(), rows, 2) for key, rows in mappings.items()],
        ]
    )


def property(entry, pool):
    registers = entry["input"]
    return [
        pool.add(entry["description"]),
        integer(registers["eax"], (1 << 32) - 1),
        integer(registers["ecx"], (1 << 32) - 1),
    ]


def cpuid(data, pool):
    vendor = property(data["vendor"], pool)
    extension = property(data["highest_extension_support"], pool)
    bits = []
    groups = {}
    registers = {name: index for index, name in enumerate(("eax", "ebx", "ecx", "edx"))}
    for key in ("flags", "extension-flags"):
        leaves = []
        for entry in data[key]:
            start = integer(len(bits))
            for bit in entry["bits"]:
                bits.append(
                    [
                        pool.add(bit["name"]),
                        registers[bit["register"].lower()],
                        integer(bit["bit"], 31),
                    ]
                )
            leaves.append([*property(entry, pool), start, integer(len(bits))])
        groups[key] = leaves
    return "\n".join(
        [
            "// Property: [description, eax, ecx]. Description is a byte offset in STRINGS.",
            f"pub(super) const VENDOR: [u32; 3] = {vendor};",
            f"pub(super) const EXTENSION: [u32; 3] = {extension};",
            "// Leaf: [description, eax, ecx, bit_start, bit_end]. Bit ranges are end-exclusive.",
            *[
                table(key.upper().replace("-", "_"), rows, 5, "u32")
                for key, rows in groups.items()
            ],
            "// [name, register (eax=0, ebx=1, ecx=2, edx=3), bit]",
            table("BITS", bits, 3),
        ]
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="check without writing")
    args = parser.parse_args()

    pool = StringPool()
    outputs = {}
    for name, generate in (("microarchitectures", architectures), ("cpuid", cpuid)):
        source = Path(f"json/cpu/{name}.json")
        data = json.loads((ROOT / source).read_text())
        outputs[name] = (source.as_posix(), generate(data, pool))
    outputs["strings"] = (
        "json/cpu/microarchitectures.json and json/cpu/cpuid.json",
        pool.render(),
    )

    for name, (source, content) in outputs.items():
        output = Path(f"src/builtin/{name}.rs")
        header = f"// @generated by scripts/generate-tables.py from {source}.\n"
        rust = subprocess.run(
            ["rustfmt", "--edition", "2018", "--emit", "stdout"],
            input=header + content + "\n",
            text=True,
            capture_output=True,
            check=True,
        ).stdout
        if args.check:
            if (ROOT / output).read_text() != rust:
                parser.exit(1, f"{output} is stale; run scripts/generate-tables.py\n")
        else:
            (ROOT / output).write_text(rust)


if __name__ == "__main__":
    main()
