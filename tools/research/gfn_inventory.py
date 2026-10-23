#!/usr/bin/env python3
"""Inventory supplied Mach-O files without executing them or modifying their bytes."""

import argparse
import hashlib
import json
import pathlib
import re
import struct


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def text(data):
    return data.split(b"\0", 1)[0].decode("utf-8", errors="replace")


def macho(data):
    if data[:4] != b"\xcf\xfa\xed\xfe":
        raise ValueError("Expected little-endian 64-bit Mach-O")
    _, cpu, subtype, kind, count, size, flags, _ = struct.unpack_from("<8I", data)
    if 32 + size > len(data):
        raise ValueError("Load commands exceed file")
    result = dict(cpu=cpu, subtype=subtype, kind=kind, flags=flags,
                  segments=[], dependencies=[], uuid=None, symbols=[])
    offset = 32
    symtab = None
    for _ in range(count):
        cmd, length = struct.unpack_from("<II", data, offset)
        if length < 8 or offset + length > 32 + size:
            raise ValueError("Invalid load command length")
        if cmd == 0x19:
            name = text(data[offset + 8:offset + 24])
            vm, vm_size, file_offset, file_size = struct.unpack_from("<4Q", data, offset + 24)
            section_count = struct.unpack_from("<I", data, offset + 64)[0]
            if 72 + section_count * 80 > length:
                raise ValueError("Sections exceed segment command")
            sections = []
            for index in range(section_count):
                start = offset + 72 + index * 80
                address, section_size, section_offset = struct.unpack_from("<QQI", data, start + 32)
                sections.append(dict(name=text(data[start:start + 16]), address=hex(address),
                                     size=section_size, file_offset=section_offset))
            result["segments"].append(dict(name=name, address=hex(vm), size=vm_size,
                                           file_offset=file_offset, file_size=file_size,
                                           sections=sections))
        elif cmd in (0xC, 0x18 | 0x80000000, 0x1F | 0x80000000, 0x20, 0x23 | 0x80000000):
            start = struct.unpack_from("<I", data, offset + 8)[0]
            result["dependencies"].append(text(data[offset + start:offset + length]))
        elif cmd == 0x1B:
            result["uuid"] = data[offset + 8:offset + 24].hex()
        elif cmd == 2:
            symtab = struct.unpack_from("<4I", data, offset + 8)
        offset += length
    if symtab:
        start, count, strings, string_size = symtab
        if start + count * 16 > len(data) or strings + string_size > len(data):
            raise ValueError("Symbol table exceeds file")
        for index in range(count):
            name_offset, symbol_type, section, desc, value = struct.unpack_from("<IBBHQ", data, start + index * 16)
            if name_offset >= string_size:
                raise ValueError("Symbol name exceeds string table")
            result["symbols"].append(dict(name=text(data[strings + name_offset:strings + string_size]),
                                           type=symbol_type, section=section, address=hex(value)))
    return result


def slices(data):
    if data[:4] == b"\xcf\xfa\xed\xfe":
        cpu = struct.unpack_from("<I", data, 4)[0]
        return [(cpu, 0, data)]
    if data[:4] not in (b"\xca\xfe\xba\xbe", b"\xca\xfe\xba\xbf"):
        return []
    wide = data[3] == 0xBF
    count = struct.unpack_from(">I", data, 4)[0]
    records = []
    for index in range(count):
        if wide:
            cpu, _, start, length, _, _ = struct.unpack_from(">IIQQII", data, 8 + index * 32)
        else:
            cpu, _, start, length, _ = struct.unpack_from(">5I", data, 8 + index * 20)
        if start + length > len(data):
            raise ValueError("Architecture slice exceeds file")
        records.append((cpu, start, data[start:start + length]))
    return records


def inventory(roots, output):
    output = output.resolve()
    for root in roots:
        if not root.is_dir():
            raise ValueError(f"Input is not a directory: {root}")
        if output == root.resolve() or root.resolve() in output.parents:
            raise ValueError("Output must be outside every input directory")
    output.mkdir(parents=True, exist_ok=True)
    records = []
    for root in roots:
        root = root.resolve()
        for path in sorted(root.rglob("*")):
            if not path.is_file() or path.is_symlink() or "__MACOSX" in path.parts:
                continue
            data = path.read_bytes()
            relative = f"{root.name}/{path.relative_to(root).as_posix()}"
            record = dict(path=relative, size=len(data), sha256=sha256(data), slices=[])
            for cpu, base, binary in slices(data):
                arch = {0x1000007: "x86_64", 0x100000C: "arm64"}.get(cpu, hex(cpu))
                meta = macho(binary)
                destination = output / "slices" / relative / arch
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(binary)
                symbols = meta.pop("symbols")
                destination.with_name(arch + ".symbols.json").write_text(json.dumps(symbols, indent=2) + "\n")
                with destination.with_name(arch + ".strings.jsonl").open("w") as stream:
                    for match in re.finditer(rb"[\x20-\x7e]{6,}", binary):
                        start = match.start()
                        segment = next((s for s in meta["segments"] if s["file_offset"] <= start < s["file_offset"] + s["file_size"]), None)
                        address = hex(int(segment["address"], 16) + start - segment["file_offset"]) if segment else None
                        stream.write(json.dumps(dict(offset=start, container_offset=base + start,
                                                     address=address, text=match.group().decode("ascii"))) + "\n")
                record["slices"].append(dict(architecture=arch, container_offset=base, size=len(binary),
                                             sha256=sha256(binary), **meta))
            records.append(record)
    (output / "inventory.json").write_text(json.dumps(records, indent=2) + "\n")
    write_markdown(records, output / "artifacts.md")
    write_field_catalog(records, output, output / "configuration-fields.md")
    print(f"Inventoried {len(records)} files and {sum(len(r['slices']) for r in records)} Mach-O slices")


def write_markdown(records, destination):
    lines = ["# Supplied artifact inventory", "",
             "Generated by `tools/research/gfn_inventory.py`. Hashes identify the supplied bytes, not a trusted vendor release.", "",
             "AppleDouble `__MACOSX` metadata and filesystem symlinks are excluded. Small regular files can be textual symlink placeholders rather than libraries.", "",
             "## Original files", "", "| Path | Bytes | SHA-256 |", "| --- | ---: | --- |"]
    for record in records:
        name = record["path"].replace("|", "\\|")
        lines.append(f"| `{name}` | {record['size']} | `{record['sha256']}` |")
    lines.extend(["", "## Architecture slices", "",
                  "Offsets below are relative to each original universal file. Function addresses in the research use the thin image's virtual address space.", "",
                  "| Path | Architecture | Container offset | Bytes | Mach-O UUID | Slice SHA-256 |",
                  "| --- | --- | ---: | ---: | --- | --- |"])
    for record in records:
        name = record["path"].replace("|", "\\|")
        for item in record["slices"]:
            lines.append(f"| `{name}` | {item['architecture']} | {item['container_offset']} | {item['size']} | `{item['uuid']}` | `{item['sha256']}` |")
    lines.extend(["", "## Linked images", "",
                  "These x86_64 load-command dependency names establish linkage only, not actual execution or successful loading. arm64 dependency equivalence is not asserted.", "",
                  "| Path | x86_64 linked images |", "| --- | --- |"])
    for record in records:
        name = record["path"].replace("|", "\\|")
        for item in record["slices"]:
            if item["architecture"] == "x86_64":
                dependencies = ", ".join("`" + dependency.replace("|", "\\|") + "`" for dependency in item["dependencies"]) or "—"
                lines.append(f"| `{name}` | {dependencies} |")
    destination.write_text("\n".join(lines) + "\n")


def write_field_catalog(records, output, destination):
    fields = {}
    for record in records:
        if pathlib.PurePosixPath(record["path"]).name != "libBifrost2.dylib":
            continue
        for item in record["slices"]:
            if item["architecture"] != "x86_64":
                continue
            sections = [section for segment in item["segments"] for section in segment["sections"]
                        if section["name"] == "__cstring"]
            strings = output / "slices" / record["path"] / "x86_64.strings.jsonl"
            for line in strings.read_text().splitlines():
                entry = json.loads(line)
                value = entry["text"]
                if not re.fullmatch(r"x-nv-[A-Za-z0-9_.\[\]-]+", value):
                    continue
                if not any(section["file_offset"] <= entry["offset"] < section["file_offset"] + section["size"] for section in sections):
                    continue
                family = re.sub(r"\[\d+\]", "[i]", value)
                field = fields.setdefault(family, dict(example=value, offset=entry["offset"], indices=set()))
                field["indices"].update(re.findall(r"\[(\d+)\]", value))
    lines = ["# NV configuration field-name catalog", "",
             "Generated from `libBifrost2.dylib`, x86_64, printable runs in `__cstring`. These are **S** observations, not recovered defaults or wire semantics.", "",
             "`[i]` groups indexed literals. The indices column lists numbers observed anywhere in those grouped literals; it does not prove valid runtime limits or all index combinations.", "",
             "The example is an exact literal. Offsets are hexadecimal offsets in the thin file. Field presence does not prove advertisement in a particular session, acceptance by a server, or implementation by OpenNOW.", "",
             "| Field or indexed family | Exact example | Observed indices | Example offset |",
             "| --- | --- | --- | --- |"]
    for family, field in sorted(fields.items()):
        indices = ", ".join(sorted(field["indices"], key=int)) or "—"
        lines.append(f"| `{family}` | `{field['example']}` | {indices} | `0x{field['offset']:x}` |")
    lines.extend(["", f"Observed {len(fields)} distinct names or indexed families. The scan selects `x-nv-` literals only; non-NV SDP, JSON settings, and other configuration namespaces are outside this catalog."])
    destination.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    inventory(args.roots, args.output)
