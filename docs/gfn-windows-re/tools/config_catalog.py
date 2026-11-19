#!/usr/bin/env python3
"""Generate the configuration-field catalog for the GFN Windows client (static + log evidence).

Inputs : ~/.capy/work/gfn-win/GFN (binaries, JSON, logs)
Needs  : pefile, capstone (uv venv at ~/.capy/work/tools/.venv)
Outputs: configuration-fields.md and nvsc-client-config.tsv in the parent folder.

Every value that could be a secret, identifier, path, or address is redacted; only key names,
enum-like literals, and numeric configuration values are emitted.
"""
import bisect, json, os, re, struct, sys
import pefile, capstone

ROOT = os.environ['GFN_DIR']
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
GER = os.path.join(ROOT, 'Geronimo.dll')
EXE = os.path.join(ROOT, 'GeForceNOW.exe')

SENSITIVE_KEY = re.compile(r'(?i)token|secret|password|pwd|ufrag|fingerprint|cert|key$|keyid|session.?id|user.?id|device|serial|mac$|ssid|email|host|address|ip$|guid|hash|acl|suffix')
SENSITIVE_VAL = re.compile(r'://|\d{1,3}(?:[.-]\d{1,3}){3}|[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-|eyJ|@|[A-Za-z0-9+/_-]{20,}|\\\\|[A-Z]:\\|Users')


def redact(key, val):
    s = str(val)
    if SENSITIVE_KEY.search(key) or SENSITIVE_VAL.search(s):
        return '<redacted>'
    return s


class PE:
    """Minimal static index: strings, imports, exports, pdata, RIP-relative references."""

    def __init__(self, path):
        self.pe = pefile.PE(path)
        self.base = self.pe.OPTIONAL_HEADER.ImageBase
        self.img = self.pe.get_memory_mapped_image()
        self.funcs = sorted((e.struct.BeginAddress, e.struct.EndAddress)
                            for e in getattr(self.pe, 'DIRECTORY_ENTRY_EXCEPTION', []))
        self.starts = [f[0] for f in self.funcs]
        self.imports = {i.address - self.base: i.name.decode() for d in getattr(self.pe, 'DIRECTORY_ENTRY_IMPORT', [])
                        for i in d.imports if i.name}
        self.imports.update({i.address - self.base: i.name.decode() for d in getattr(self.pe, 'DIRECTORY_ENTRY_DELAY_IMPORT', [])
                             for i in d.imports if i.name})
        self.exports = {s.address: s.name.decode() for s in getattr(getattr(self.pe, 'DIRECTORY_ENTRY_EXPORT', None), 'symbols', []) if s.name}
        self.strings = {}
        for sec in self.pe.sections:
            if sec.Characteristics & 0x20000000:
                continue
            va = sec.VirtualAddress
            buf = self.img[va: va + sec.Misc_VirtualSize]
            for m in re.finditer(rb'[\x20-\x7e]{3,}\x00', buf):
                self.strings[va + m.start()] = m.group()[:-1].decode()
            for m in re.finditer(rb'(?:[\x20-\x7e]\x00){4,}\x00\x00', buf):
                self.strings.setdefault(va + m.start(), m.group()[:-2].decode('utf-16le'))
        self.refs = {}
        self.calls = set()
        for sec in self.pe.sections:
            if not sec.Characteristics & 0x20000000:
                continue
            va = sec.VirtualAddress
            buf = self.img[va: va + sec.Misc_VirtualSize]
            for m in re.finditer(rb'[\x48\x4c][\x8d\x8b][\x05\x0d\x15\x1d\x25\x2d\x35\x3d]', buf):
                o = m.start()
                self.refs.setdefault(va + o + 7 + struct.unpack_from('<i', buf, o + 3)[0], []).append(va + o)
            for m in re.finditer(rb'\xff[\x15\x25]', buf):
                o = m.start()
                self.refs.setdefault(va + o + 6 + struct.unpack_from('<i', buf, o + 2)[0], []).append(va + o)
            for m in re.finditer(rb'\xe8', buf):
                o = m.start()
                if o + 5 <= len(buf):
                    self.calls.add(va + o + 5 + struct.unpack_from('<i', buf, o + 1)[0])
        self.md = capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64)

    def va(self, rva):
        return hex(self.base + rva)

    def func_extent(self, rva):
        """Function start plus following pdata fragments until the next real call target."""
        i = bisect.bisect_right(self.starts, rva) - 1
        lo, hi = self.funcs[i]
        j = i + 1
        while j < len(self.funcs) and self.funcs[j][0] not in self.calls and self.funcs[j][0] not in self.exports \
                and self.funcs[j][0] - hi < 0x40:
            hi = self.funcs[j][1]
            j += 1
        return lo, hi

    def strings_used_by(self, rva):
        lo, hi = self.func_extent(rva)
        out = []
        for tgt, srcs in self.refs.items():
            if tgt in self.strings and any(lo <= s < hi for s in srcs):
                out.append((tgt, self.strings[tgt]))
        return sorted(out)

    def find_strings(self, rx):
        r = re.compile(rx)
        return [(a, s) for a, s in sorted(self.strings.items()) if r.search(s)]

    def struct_tag_before_import_call(self, func_rva, import_name):
        """Return the immediate stored at the struct passed in r9 before calling import_name (first field)."""
        lo, hi = self.func_extent(func_rva)
        insns = list(self.md.disasm(self.img[lo:hi], self.base + lo))
        tags = []
        for k, ins in enumerate(insns):
            m = re.search(r'\[rip \+ (0x[0-9a-f]+)\]', ins.op_str)
            if ins.mnemonic != 'call' or not m:
                continue
            if self.imports.get(ins.address + ins.size + int(m.group(1), 16) - self.base) != import_name:
                continue
            window = insns[max(0, k - 40):k]
            slot = next((re.search(r'\[rsp \+ (0x[0-9a-f]+)\]', w.op_str).group(1) for w in reversed(window)
                         if w.mnemonic == 'lea' and w.op_str.startswith('r9,') and 'rsp' in w.op_str), None)
            imm, how = None, ''
            stores = [re.match(r'(?:dword|qword) ptr \[rsp \+ (0x[0-9a-f]+)\], (0x[0-9a-f]+|\d+)$', w.op_str)
                      for w in window if w.mnemonic == 'mov']
            stores = [(int(m.group(1), 16), int(m.group(2), 0)) for m in stores if m]
            for off, val in stores:
                if slot and off == int(slot, 16):
                    imm, how = val, 'D'
            if imm is None and slot:
                for off, val in stores:
                    if off == int(slot, 16) - 0x20:
                        imm, how = val, 'D (struct staged 0x20 below the r9 slot)'
            tags.append((ins.address, imm, how))
        return tags


def md_table(rows, header):
    out = ['| ' + ' | '.join(header) + ' |', '|' + '---|' * len(header)]
    out += ['| ' + ' | '.join(str(c).replace('|', '\\|') for c in r) + ' |' for r in rows]
    return '\n'.join(out)


def main():
    doc = ['# Configuration fields (GFN Windows 2.0.89.141)', '',
           'Names plus evidence only; no semantics are claimed beyond what a cited string, disassembly, or log value shows. '
           'macOS-derived names are not used here; compare with the macOS `configuration-fields.md` (PR #1161) separately.', '',
           'Generated by `config_catalog.py`. Evidence: **S** = string/data present in the named binary at the given VA; '
           '**D** = recovered from disassembly at the given VA; **L** = value read from a supplied log/JSON file. '
           'Values that could identify the user or machine are `<redacted>`.', '']
    ger, exe = PE(GER), PE(EXE)

    # 1. GeForceNOW.json switches and the nv-* switch vocabulary in GeForceNOW.exe
    switches = json.load(open(os.path.join(ROOT, 'GeForceNOW.json')))['switches']
    exe_nv = {s: a for a, s in exe.find_strings(r'^nv-[a-z0-9-]+$')}
    rows = []
    for sw in switches:
        name, _, val = sw.partition('=')
        rows.append((f'`{name}`', redact(name, val), exe.va(exe_nv[name]) if name in exe_nv else 'not found as literal'))
    doc += ['## 1. GeForceNOW.json `switches` (L) and their literal in GeForceNOW.exe (S)', '',
            md_table(rows, ['switch', 'value in GeForceNOW.json', 'GeForceNOW.exe literal VA']), '']
    unused = sorted(n for n in exe_nv if n not in {s.partition('=')[0] for s in switches})
    doc += [f'Other `nv-*` literals in GeForceNOW.exe not set by this GeForceNOW.json ({len(unused)}; S):', '',
            ', '.join(f'`{n}` ({exe.va(exe_nv[n])})' for n in unused), '']

    # 2. Native query names (JS -> native) in GeForceNOW.exe
    q = exe.find_strings(r'^QUERY_GFN_[A-Z0-9_]+$')
    doc += [f'## 2. Native query names in GeForceNOW.exe ({len(q)}; S)', '',
            ', '.join(f'`{s}`' for _, s in q), '']

    # 3. Server-type conversion literals (GeForceNOW.exe)
    st = exe.find_strings(r'Server type conversion returning NVB_SERVER_')
    doc += ['## 3. Server-type conversion literals in GeForceNOW.exe (S)', '',
            md_table([(exe.va(a), f'`{s.split()[-1]}`') for a, s in st], ['VA', 'name']), '']

    # 4. Geronimo remote config (GXT) keys and local debug-config keys
    for sub, (title, rva) in zip('ab', [('GXT remote-config loader `0x180106f90` (keys read from `remoteOverrides`/`geronimo`/`nvst`/`pmCommunication`)', 0x106f90),
                       ('Local debug config loader `0x18010f360` (`GeronimoDebugConfig.txt`)', 0x10f360)]):
        keys = [(ger.va(a), s) for a, s in ger.strings_used_by(rva) if re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{1,60}', s)]
        logs = [s for a, s in ger.strings_used_by(rva) if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{1,60}', s)]
        doc += [f'## 4{sub}. Geronimo: {title}', '',
                f'Identifier-like literals referenced by the function (S/D, {len(keys)}):', '',
                ', '.join(f'`{s}`' for _, s in keys), '',
                'Log/format literals in the same function (S): ' + '; '.join(f'"{s[:90]}"' for s in logs[:12]), '']

    # 5. GeronimoSettingsImpl / GeronimoDebugConfig exported accessors
    acc = sorted({n.split('@')[0].lstrip('?') for n in ger.exports.values()
                  if '@GeronimoSettingsImpl@@' in n or '@GeronimoDebugConfig@@' in n})
    doc += [f'## 5. Geronimo `GeronimoSettingsImpl` / `GeronimoDebugConfig` exported accessors ({len(acc)}; S)', '',
            ', '.join(f'`{a}`' for a in acc), '']

    # 6. nvbFeatureControl / nvbSendClientInfo tags per BifrostSDKExecutor method
    rows = []
    for rva, name in sorted(ger.exports.items()):
        if '@BifrostSDKExecutor@@' not in name:
            continue
        meth = name.split('@')[0].lstrip('?')
        for imp in ('nvbFeatureControl', 'nvbSendClientInfo'):
            for addr, imm, how in ger.struct_tag_before_import_call(rva, imp):
                rows.append((f'`{meth}`', ger.va(rva), imp, hex(addr),
                             'not an immediate here (argument or computed)' if imm is None else f'{imm} ({how})'))
    doc += ['## 6. Bifrost feature/client-info tags set by Geronimo `BifrostSDKExecutor` (D)', '',
            'Tag = immediate stored into the first field of the struct passed (in r9) to the import. '
            'geronimo.log(+.bak) also prints `Disabled feature type: 0` and `Update feature type: 10` (L); feature type 13 (`Disabled feature type: 13`) '
            'has no method in this table, and feature type 0 is the value `controlFeatures` passes through from its caller (U).', '',
            md_table(rows, ['method', 'method VA', 'import', 'call VA', 'tag']), '']

    # 7. Remote-config gated features and other config literals in Geronimo
    rc = ger.find_strings(r'RemoteConfig|remote config|GXT')
    doc += ['## 7. Geronimo literals that mention RemoteConfig/GXT (S)', '',
            md_table([(ger.va(a), s[:120]) for a, s in rc], ['VA', 'literal']), '']

    # 8. SDL hints and registry keys
    sdl = ger.find_strings(r'^SDL_[A-Z0-9_]+$') + exe.find_strings(r'^SDL_[A-Z0-9_]+$')
    reg = ger.find_strings(r'(?i)^(software|system)\\|NVIDIA Corporation\\') + exe.find_strings(r'(?i)^(software|system)\\|NVIDIA Corporation\\')
    doc += ['## 8. SDL hint literals and registry paths (S)', '',
            'SDL hint names: ' + (', '.join(sorted({f'`{s}`' for _, s in sdl})) or 'none'), '',
            'Registry-like paths: ' + ', '.join(sorted({f'`{s}`' for _, s in reg})), '']

    # 9. JSON files: key names only
    doc += ['## 9. Local JSON/config files (L; key names only)', '']
    for rel in ['storage.json', 'updatestatus.json', 'nvc/localuser/NvTelemetry.json',
                'nvc/localuser/GFN/GeForceNOWReliabilityMonitor.json']:
        try:
            d = json.load(open(os.path.join(ROOT, rel)))
        except Exception as e:
            doc += [f'- `{rel}`: unreadable ({type(e).__name__})']
            continue
        def keys(x, p=''):
            if isinstance(x, dict):
                for k, v in x.items():
                    k2 = '<opaque-id-key>' if len(k) > 24 or '\\' in k else k
                    yield from keys(v, f'{p}.{k2}' if p else k2)
            else:
                yield p
        doc += [f'- `{rel}`: ' + ', '.join(f'`{k}`' for k in keys(d))]
    doc += ['']

    # 10. AppConfigs/v2 schema summary
    doc += ['## 10. AppConfigs/v2 (L; structure only)', '']
    for fn in sorted(os.listdir(os.path.join(ROOT, 'AppConfigs/v2'))):
        raw = open(os.path.join(ROOT, 'AppConfigs/v2', fn), encoding='utf-8', errors='replace').read()
        try:
            d = json.loads(raw)
        except json.JSONDecodeError as e:
            top = re.findall(r'^\s{2}"([A-Za-z$][\w$]*)"\s*:', raw, re.M)
            doc += [f'- `{fn}`: not strict JSON ({e.msg} at line {e.lineno}); top-level keys by regex: '
                    + ', '.join(f'`{k}`' for k in dict.fromkeys(top))]
            continue
        parts = []
        for k, v in d.items():
            if isinstance(v, list):
                parts.append(f'`{k}`[{len(v)}]')
            elif isinstance(v, dict):
                parts.append(f'`{k}`{{{len(v)}}}')
            else:
                parts.append(f'`{k}`')
        doc += [f'- `{fn}`: ' + ', '.join(parts)]
    doc += ['']

    # 11. NvscClientConfig dump from geronimo.log.bak (final client config logged by Bifrost/NVST)
    rows = []
    for ln in open(os.path.join(ROOT, 'geronimo.log.bak'), errors='replace'):
        m = re.search(r'\[NVST:NvscClientConfig\] \{\w+\} -\s+([\w.\[\]]+): (.*?)\s*$', ln)
        if m:
            rows.append((m.group(1), redact(m.group(1), m.group(2))))
    with open(os.path.join(OUT, 'nvsc-client-config.tsv'), 'w') as f:
        f.write('key\tvalue\n')
        for k, v in rows:
            f.write(f'{k}\t{v}\n')
    groups = {}
    for k, _ in rows:
        groups[k.split('.')[0]] = groups.get(k.split('.')[0], 0) + 1
    doc += ['## 11. NvscClientConfig dump (L, geronimo.log.bak, S6, 19:45:31 local)', '',
            f'{len(rows)} `key: value` lines written to `nvsc-client-config.tsv`. Top-level groups: '
            + ', '.join(f'`{g}` ({n})' for g, n in sorted(groups.items())), '']

    open(os.path.join(OUT, 'configuration-fields.md'), 'w').write('\n'.join(doc) + '\n')
    print('wrote', os.path.join(OUT, 'configuration-fields.md'), 'and nvsc-client-config.tsv', len(rows), 'config rows')


if __name__ == '__main__':
    main()
