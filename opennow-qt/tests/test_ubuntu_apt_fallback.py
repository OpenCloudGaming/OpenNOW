import functools
import getpass
import hashlib
import os
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest


@unittest.skipUnless(sys.platform.startswith("linux") and all(shutil.which(tool) for tool in
                     ("apt-get", "apt-ftparchive", "dpkg-deb", "gpg", "gpgconf")), "Requires Debian repository tools")
class UbuntuAptFallbackTest(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        repository = self.root / "repository"
        package = self.root / "package"
        (package / "DEBIAN").mkdir(parents=True)
        (package / "DEBIAN/control").write_text(
            "Package: opennow-apt-fixture\nVersion: 1.0\nArchitecture: all\n"
            "Maintainer: OpenNOW tests <test@example.invalid>\nDescription: signed APT fixture\n")
        (package / "fixture").write_text("Downloaded through APT mirror failover\n")
        (repository / "pool").mkdir(parents=True)
        self.run_command(["dpkg-deb", "--build", str(package), str(repository / "pool/fixture.deb")])
        index = repository / "dists/noble/main/binary-amd64"
        index.mkdir(parents=True)
        (index / "Packages").write_bytes(self.run_command(
            ["apt-ftparchive", "packages", "pool"], cwd=repository).stdout)
        package_index = (index / "Packages").read_bytes()
        for algorithm in ("sha256", "sha512"):
            hashes = index / "by-hash" / algorithm.upper()
            hashes.mkdir(parents=True)
            (hashes / hashlib.new(algorithm, package_index).hexdigest()).write_bytes(package_index)
        release = repository / "dists/noble/Release"
        release.write_bytes(self.run_command(
            ["apt-ftparchive", "-o", "APT::FTPArchive::Release::Codename=noble",
             "-o", "APT::FTPArchive::Release::Suite=noble", "release", "dists/noble"], cwd=repository).stdout)
        keyring = self.root / "gnupg"
        keyring.mkdir(mode=0o700)
        self.addCleanup(subprocess.run, ["gpgconf", "--homedir", str(keyring), "--kill", "gpg-agent"], capture_output=True)
        gpg = ["gpg", "--homedir", str(keyring), "--batch", "--pinentry-mode", "loopback", "--passphrase", ""]
        self.run_command([*gpg, "--quick-generate-key", "OpenNOW APT test", "ed25519", "sign", "0"])
        public = self.root / "archive-keyring.gpg"
        public.write_bytes(self.run_command([*gpg, "--export"]).stdout)
        self.run_command([*gpg, "--clearsign", "--output", str(release.with_name("InRelease")), str(release)])
        self.requests = []
        requests = self.requests

        class Handler(SimpleHTTPRequestHandler):
            def log_message(self, format, *args):
                return

            def do_GET(self):
                requests.append(self.path)
                prefix, _, path = self.path.lstrip("/").partition("/")
                if prefix == "offline":
                    self.send_error(503)
                    return
                if prefix == "invalid" and "/by-hash/" in path:
                    self.send_error(404)
                    return
                if prefix == "tampered" and "/by-hash/" in path:
                    content = b"x" * len(package_index)
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(content)))
                    self.end_headers()
                    self.wfile.write(content)
                    return
                if prefix == "invalid" and path.endswith("/Packages"):
                    content = b"Corrupt package index\n"
                    self.send_response(200)
                    self.send_header("Content-Length", str(len(content)))
                    self.end_headers()
                    self.wfile.write(content)
                    return
                self.path = "/" + path
                super().do_GET()

        server = ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(repository)))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(thread.join)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        self.base = f"http://127.0.0.1:{server.server_port}"
        self.mirrors = self.root / "mirrors.txt"
        sources = self.root / "sources.list"
        sources.write_text(f"deb [arch=amd64 signed-by={public} by-hash=force] mirror+file:{self.mirrors} noble main\n")
        (self.root / "lists/partial").mkdir(parents=True)
        config = self.root / "apt.conf"
        config.write_text('Dir::Etc::parts "-";\nDir::Etc::main "-";\n')
        self.apt_env = {**os.environ, "APT_CONFIG": str(config)}
        self.options = ["-o", f"Dir::Etc::sourcelist={sources}", "-o", "Dir::Etc::sourceparts=-",
                        "-o", f"Dir::State::lists={self.root / 'lists'}", "-o", "Acquire::Languages=none",
                        "-o", f"APT::Sandbox::User={getpass.getuser()}", "-o", "Acquire::Retries=0",
                        "-o", "Acquire::http::Timeout=3"]

    def run_command(self, command, **kwargs):
        result = subprocess.run(command, capture_output=True, timeout=30, **kwargs)
        self.assertEqual(result.returncode, 0, result.stdout.decode(errors="replace") + result.stderr.decode(errors="replace"))
        return result

    def check_fallback(self, first):
        self.mirrors.write_text(f"{self.base}/{first}\tpriority:1\n{self.base}/healthy\tpriority:2\n")
        self.run_command(["apt-get", *self.options, "update", "--error-on=any"], env=self.apt_env)
        self.run_command(["apt-get", *self.options, "download", "opennow-apt-fixture"], cwd=self.root, env=self.apt_env)
        downloaded = self.root / "opennow-apt-fixture_1.0_all.deb"
        self.assertEqual(downloaded.read_bytes(), (self.root / "repository/pool/fixture.deb").read_bytes())
        self.assertTrue(any(path.startswith(f"/{first}/") for path in self.requests))
        self.assertTrue(any(path.startswith("/healthy/") for path in self.requests))
        self.assertFalse(any(path.endswith("/Packages") for path in self.requests))

    def test_signed_repository_and_package_download_fall_back_from_offline_mirror(self):
        self.check_fallback("offline")

    def test_signed_repository_and_package_download_fall_back_from_invalid_index(self):
        self.check_fallback("invalid")

    def test_hash_mismatch_cannot_be_accepted_as_an_authenticated_index(self):
        self.mirrors.write_text(f"{self.base}/tampered\tpriority:1\n{self.base}/healthy\tpriority:2\n")
        result = subprocess.run(["apt-get", *self.options, "update", "--error-on=any"],
                                capture_output=True, text=True, timeout=30, env=self.apt_env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Hash Sum mismatch", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
