import argparse
import hashlib
import html
from html.parser import HTMLParser
from pathlib import Path, PurePosixPath
import re
import subprocess
from urllib.parse import unquote, urlsplit

import markdown


ROOT = Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs/gfn-official-binary-audit"
SECTIONS = [
    ("README", "Overview"),
    ("manifest-and-binaries", "Manifest & binaries"),
    ("session-creation-cloudmatch", "Session & CloudMatch"),
    ("transport-rtsp-mjolnir", "Transport (RTSP / Mjolnir)"),
    ("video-streaming-decode-recovery", "Video decode & recovery"),
    ("audio-opus-red-jitter", "Audio"),
    ("input-mouse-gamepad-features", "Input & features"),
    ("opennow-parity-gaps", "Parity gaps"),
]
SOURCE_CHECKS = {
    "native/opennow-streamer/crates/opennow-streamer-ffi/src/lib.rs": [
        "Engine::with_embedded_media_runtime_and_hid", "create_embedded_runtime_with_config",
    ],
    "native/opennow-streamer/crates/opennow-streamer-platform/src/runtime.rs": [
        "MediaSession::spawn_embedded",
    ],
    "native/opennow-streamer/crates/opennow-streamer-platform/src/graphics.rs": [
        "scene.latest.replace(FrameSlot",
    ],
    "opennow-qt/src/streaming/NativeStreamRuntime.cpp": [
        "d->api.acquireLatestFrame(d->handle", "d->api.recordFrame(d->handle",
    ],
    "native/opennow-streamer/crates/opennow-streamer-platform/src/microphone.rs": [
        "opus::Encoder::new", "sys::SDL_OpenAudioDevice",
    ],
    "native/opennow-streamer/crates/opennow-streamer-core/src/microphone.rs": [
        ".send_microphone_opus(frame.opus, frame.timestamp)",
    ],
    "native/opennow-streamer/crates/opennow-streamer-transport/src/nvst.rs": [
        "if upstream_allowed && dtls_ready", "!mjolnir && rtcp_channel_open",
    ],
    "native/opennow-streamer/crates/opennow-streamer-transport/src/nvst_input.rs": [
        "&NVST_CHANNEL_PROFILE[..6]", "bundle_video.then(||",
    ],
}


class ReportHTML(HTMLParser):
    def __init__(self, template=False):
        super().__init__()
        self.ids = set()
        self.hrefs = []
        self.allowed = {
            tag: {"id", "class"} for tag in (
                "section", "h1", "h2", "h3", "h4", "h5", "h6", "p", "strong", "em",
                "code", "pre", "table", "thead", "tbody", "tr", "ul", "ol", "li",
                "blockquote", "hr", "br", "del",
            )
        }
        self.allowed.update({"a": {"id", "class", "href", "title"}, "th": {"id", "class", "style"}, "td": {"id", "class", "style"}})
        if template:
            self.allowed.update({tag: {"id", "class"} for tag in ("head", "title", "body", "div", "nav", "main")})
            self.allowed["style"] = set()
            self.allowed.update({"html": {"lang"}, "meta": {"charset", "name", "content"}})

    def handle_starttag(self, tag, attrs):
        if tag not in self.allowed:
            raise ValueError(f"Unsupported HTML element: {tag}")
        keys = [key for key, _ in attrs]
        if len(set(keys)) != len(keys):
            raise ValueError(f"Duplicate HTML attribute on {tag}")
        if set(keys) - self.allowed[tag]:
            raise ValueError(f"Unsupported HTML attribute on {tag}")
        attrs = dict(attrs)
        if "style" in attrs:
            alignment = re.sub(r"\s", "", attrs["style"])
            if tag not in {"th", "td"} or alignment not in {"text-align:left;", "text-align:right;", "text-align:center;"}:
                raise ValueError(f"Unsupported inline style on {tag}")
        if "id" in attrs:
            if attrs["id"] in self.ids:
                raise ValueError(f"Duplicate HTML id: {attrs['id']}")
            self.ids.add(attrs["id"])
        if "href" in attrs:
            self.hrefs.append(attrs["href"])


def bounded_text(path):
    if path.stat().st_size > 2 * 1024 * 1024:
        raise ValueError(f"Audit source exceeds 2 MiB: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def git_source(ref, path):
    size = subprocess.check_output(["git", "cat-file", "-s", f"{ref}:{path}"], cwd=ROOT, text=True)
    if int(size) > 2 * 1024 * 1024:
        raise ValueError(f"Comparison source exceeds 2 MiB: {path}")
    result = subprocess.run(
        ["git", "show", f"{ref}:{path}"], cwd=ROOT, check=True,
        stdout=subprocess.PIPE, text=True,
    )
    return result.stdout


def check_sources(overview):
    match = re.search(r"\| OpenNOW source comparison \| `([0-9a-f]{40})`", overview)
    if not match:
        raise ValueError("Overview must name the exact OpenNOW comparison commit")
    ref = match[1]
    for path, needles in SOURCE_CHECKS.items():
        source = git_source(ref, path)
        for needle in needles:
            if needle not in source:
                raise ValueError(f"Recheck documented ownership: {path}: {needle}")
    source = git_source(ref, "native/opennow-streamer/crates/opennow-streamer-core/src/nvst_rtsp.rs")
    body = source.split("fn build_announce(", 1)[1].split("\n}\n", 1)[0]
    for attribute in ("aqos.enableRedundancy", "aqos.redundancyLevel", "enetControlChannel.mtuSize", "rtpNackVersion"):
        if attribute in body:
            raise ValueError(f"Recheck documented ANNOUNCE fields: {attribute}")
    for attribute in ("drc.enable:0", "resControl.bitrateIirFilterFactor:128"):
        if attribute not in body:
            raise ValueError(f"Recheck documented ANNOUNCE fields: {attribute}")
    if "omit_server_announce_attributes" not in source:
        raise ValueError("Recheck server-owned ANNOUNCE filtering")
    return ref


def check_links(hrefs):
    count = 0
    for href in hrefs:
        url = urlsplit(href)
        if url.scheme and url.scheme not in {"https", "http"}:
            raise ValueError(f"Unsupported link scheme: {url.scheme}")
        if url.username or url.password:
            raise ValueError("Credential-bearing link")
        if url.scheme or not url.path:
            continue
        path = (DOCS / PurePosixPath(unquote(url.path))).resolve()
        if not path.is_relative_to(ROOT) or not path.is_file():
            raise ValueError(f"Missing or out-of-repository link: {href}")
        count += 1
    return count


def main():
    parser = argparse.ArgumentParser(description="Render and check the bounded static GFN audit corpus.")
    parser.add_argument("--check", action="store_true", help="Require the committed HTML to match without writing it.")
    args = parser.parse_args()
    if markdown.__version__ != "3.10.2":
        raise ValueError("Install the pinned docs/gfn-official-binary-audit/requirements.txt")
    sources = {name: bounded_text(DOCS / f"{name}.md") for name, _ in SECTIONS}
    comparison = check_sources(sources["README"])
    navigation = "".join(f'<li><a href="#{name}">{html.escape(title)}</a></li>' for name, title in SECTIONS)
    sections = []
    for name, title in SECTIONS:
        section = f'<section id="{name}"><h1 class="section-title">{html.escape(title)}</h1>'
        section += markdown.markdown(sources[name], extensions=["tables", "fenced_code"]) + "</section>"
        ReportHTML().feed(section)
        sections.append(section)
    template = bounded_text(DOCS / "report-template.html")
    styles = re.findall(r"<style>(.*?)</style>", template, re.DOTALL)
    if re.findall(r"<style\b[^>]*>", template, re.IGNORECASE) != ["<style>"] or re.findall(r"</style\s*>", template, re.IGNORECASE) != ["</style>"]:
        raise ValueError("Template must contain exactly one reviewed stylesheet")
    if len(styles) != 1 or hashlib.sha256(styles[0].encode("utf-8")).hexdigest() != "117dfc069d79817820e048846df79e9b7a278229bd693ca97ace3667dd1d311a":
        raise ValueError("Template must use the reviewed passive stylesheet")
    for marker in ("{{navigation}}", "{{sections}}"):
        if template.count(marker) != 1:
            raise ValueError(f"Template needs exactly one {marker}")
    report = template.replace("{{navigation}}", navigation).replace("{{sections}}", "".join(sections))
    document = ReportHTML(template=True)
    document.feed(report)
    missing = [href for href in document.hrefs if href.startswith("#") and href[1:] not in document.ids]
    if missing:
        raise ValueError(f"Missing HTML anchors: {missing}")
    links = check_links(document.hrefs)
    for name, source in sources.items():
        if re.search(r"eyJ[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{15,}\.[A-Za-z0-9_-]{10,}", source):
            raise ValueError(f"Possible JWT in {name}; inspect without printing its value")
        if re.search(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,})", source):
            raise ValueError(f"Possible private credential in {name}")
    destination = DOCS / "index.html"
    if args.check:
        if bounded_text(destination) != report:
            raise ValueError("index.html differs; regenerate it before publishing")
    else:
        destination.write_text(report, encoding="utf-8")
    print(f"Audit {'checked' if args.check else 'generated'}: {len(sources)} sections, {links} relative links, comparison {comparison}")


if __name__ == "__main__":
    main()
