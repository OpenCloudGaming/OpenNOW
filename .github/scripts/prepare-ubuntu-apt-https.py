import os
from pathlib import Path
import re
import stat
import sys
import tempfile
from urllib.parse import urlsplit


OFFICIAL_MIRRORS = {
    "archive.ubuntu.com",
    "security.ubuntu.com",
    "us.archive.ubuntu.com",
}
FALLBACK_MIRRORS = "https://archive.ubuntu.com/ubuntu\tpriority:1\nhttps://security.ubuntu.com/ubuntu\tpriority:2\n"


def https_uri(uri):
    parsed = urlsplit(uri)
    if (parsed.scheme not in ("http", "https") or parsed.path not in ("/ubuntu", "/ubuntu/")
            or parsed.query or parsed.fragment):
        return uri
    if parsed.netloc in ("mirrors.sonic.net", "mirrors.edge.kernel.org"):
        return "https://archive.ubuntu.com" + parsed.path
    if parsed.scheme == "http" and parsed.netloc in OFFICIAL_MIRRORS:
        return "https" + uri[4:]
    return uri


def rewrite(text, format, transform=https_uri):
    lines = []
    in_uris = False
    for line in text.splitlines(keepends=True):
        content, marker, comment = line.partition("#")
        if not content.strip():
            if not marker:
                in_uris = False
            lines.append(line)
            continue
        if format == "sources":
            field = re.match(r"^(URIs:)(.*)$", content, re.IGNORECASE | re.DOTALL)
            if field:
                in_uris = True
                content = field[1] + re.sub(r"\S+", lambda match: transform(match[0]), field[2])
            elif in_uris and content[0].isspace():
                content = re.sub(r"\S+", lambda match: transform(match[0]), content)
            else:
                in_uris = False
        else:
            pattern = r"^(\s*deb(?:-src)?\s+(?:\[[^\]\r\n]*\]\s+)?)(\S+)" if format == "list" else r"^(\s*)(\S+)"
            match = re.match(pattern, content)
            if match:
                content = content[:match.start(2)] + transform(match[2]) + content[match.end(2):]
        lines.append(content + marker + comment)
    return "".join(lines)


def replace(path, updated):
    if path.is_symlink():
        raise ValueError(f"Refusing to replace a linked apt source: {path}")
    metadata = path.stat() if path.exists() else None
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="",
                                         dir=path.parent, delete=False) as target:
            temporary = Path(target.name)
            os.fchmod(target.fileno(), stat.S_IMODE(metadata.st_mode) if metadata else 0o644)
            if metadata:
                os.fchown(target.fileno(), metadata.st_uid, metadata.st_gid)
            target.write(updated)
            target.flush()
            os.fsync(target.fileno())
        temporary.replace(path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def require_immutable_indexes(text, format, mirror_uri):
    if format == "sources":
        parts = re.split(r"(\r?\n[ \t]*\r?\n)", text)
        for index, part in enumerate(parts):
            uris = re.search(r"(?im)^URIs:[^\r\n]*(?:\r?\n[ \t]+[^\r\n]*)*", part)
            if not uris or mirror_uri not in uris[0].split():
                continue
            if re.search(r"(?im)^By-Hash:", part):
                parts[index] = re.sub(r"(?im)^(By-Hash:)[^\r\n]*", r"\1 force", part)
            else:
                newline = "\r\n" if "\r\n" in part else "\n"
                ending = newline if part.endswith(newline) else ""
                parts[index] = part.removesuffix(ending) + newline + "By-Hash: force" + ending
        return "".join(parts)
    lines = []
    for line in text.splitlines(keepends=True):
        match = re.match(r"^(\s*deb(?:-src)?\s+)(?:\[([^\]\r\n]*)\]\s+)?(\S+)", line)
        if match and match[3] == mirror_uri:
            options = [option for option in (match[2] or "").split() if not option.startswith("by-hash=")]
            options.append("by-hash=force")
            line = match[1] + "[" + " ".join(options) + "] " + line[match.start(3):]
        lines.append(line)
    return "".join(lines)


def prepare(apt_dir=Path("/etc/apt")):
    mirrors = apt_dir / "opennow-ubuntu-mirrors.txt"
    mirror_uri = "mirror+file:" + str(mirrors)
    uses_mirrors = False

    def source_uri(uri):
        nonlocal uses_mirrors
        canonical = https_uri(uri)
        parsed = urlsplit(canonical)
        if (uri in (mirror_uri, "mirror+file:/etc/apt/blacksmith-ubuntu-mirrors.txt")
                or (parsed.scheme == "https" and parsed.netloc in OFFICIAL_MIRRORS
                    and parsed.path in ("/ubuntu", "/ubuntu/") and not parsed.query and not parsed.fragment)):
            uses_mirrors = True
            return mirror_uri
        return uri

    candidates = [(apt_dir / "sources.list", "list")]
    for format in ("list", "sources"):
        candidates.extend((path, format) for path in sorted((apt_dir / "sources.list.d").glob(f"*.{format}")))
    updates = []
    for path, format in candidates:
        if not path.exists():
            continue
        with path.open(encoding="utf-8", newline="") as source:
            original = source.read()
        updated = rewrite(original, format, source_uri)
        updated = require_immutable_indexes(updated, format, mirror_uri)
        if updated == original:
            continue
        updates.append((path, updated))
    if uses_mirrors and mirrors.is_symlink():
        raise ValueError(f"Refusing to replace a linked apt source: {mirrors}")
    if uses_mirrors and (not mirrors.exists() or mirrors.read_text() != FALLBACK_MIRRORS):
        updates.insert(0, (mirrors, FALLBACK_MIRRORS))
    for path, _ in updates:
        if path.is_symlink():
            raise ValueError(f"Refusing to replace a linked apt source: {path}")
    for path, updated in updates:
        replace(path, updated)
    return [path for path, _ in updates]


if __name__ == "__main__":
    for path in prepare(Path(sys.argv[1]) if len(sys.argv) > 1 else Path("/etc/apt")):
        print(f"Enabled verified Ubuntu HTTPS mirror failover in {path}")
