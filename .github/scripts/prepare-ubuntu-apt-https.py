import os
from pathlib import Path
import re
import stat
import tempfile
from urllib.parse import urlsplit


OFFICIAL_MIRRORS = {
    "archive.ubuntu.com",
    "security.ubuntu.com",
    "us.archive.ubuntu.com",
}


def https_uri(uri):
    parsed = urlsplit(uri)
    if (parsed.scheme != "http" or parsed.path not in ("/ubuntu", "/ubuntu/")
            or parsed.query or parsed.fragment):
        return uri
    if parsed.netloc == "mirrors.sonic.net":
        return "https://archive.ubuntu.com" + parsed.path
    if parsed.netloc in OFFICIAL_MIRRORS:
        return "https" + uri[4:]
    return uri


def rewrite(text, format):
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
                content = field[1] + re.sub(r"\S+", lambda match: https_uri(match[0]), field[2])
            elif in_uris and content[0].isspace():
                content = re.sub(r"\S+", lambda match: https_uri(match[0]), content)
            else:
                in_uris = False
        else:
            pattern = r"^(\s*deb(?:-src)?\s+(?:\[[^\]\r\n]*\]\s+)?)(\S+)" if format == "list" else r"^(\s*)(\S+)"
            match = re.match(pattern, content)
            if match:
                content = content[:match.start(2)] + https_uri(match[2]) + content[match.end(2):]
        lines.append(content + marker + comment)
    return "".join(lines)


def prepare(apt_dir=Path("/etc/apt")):
    candidates = [(apt_dir / "sources.list", "list"),
                  (apt_dir / "blacksmith-ubuntu-mirrors.txt", "mirrors")]
    for format in ("list", "sources"):
        candidates.extend((path, format) for path in sorted((apt_dir / "sources.list.d").glob(f"*.{format}")))
    changed = []
    for path, format in candidates:
        if not path.exists():
            continue
        with path.open(encoding="utf-8", newline="") as source:
            original = source.read()
        updated = rewrite(original, format)
        if updated == original:
            continue
        if path.is_symlink():
            raise ValueError(f"Refusing to replace a linked apt source: {path}")
        metadata = path.stat()
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="",
                                             dir=path.parent, delete=False) as target:
                temporary = Path(target.name)
                os.fchmod(target.fileno(), stat.S_IMODE(metadata.st_mode))
                os.fchown(target.fileno(), metadata.st_uid, metadata.st_gid)
                target.write(updated)
                target.flush()
                os.fsync(target.fileno())
            temporary.replace(path)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
        changed.append(path)
    return changed


if __name__ == "__main__":
    for path in prepare():
        print(f"Enabled verified Ubuntu HTTPS mirror transport in {path}")
