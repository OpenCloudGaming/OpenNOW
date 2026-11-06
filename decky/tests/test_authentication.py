import base64
import copy
import hashlib
import io
import json
import shutil
import subprocess
import tempfile
import threading
import unittest
import urllib.request
from pathlib import Path
from unittest import mock

from tests import support
from opennow_decky import host, installer, net


class Response(io.BytesIO):
    def __init__(self, data):
        super().__init__(data)
        self.headers = {"Content-Length": str(len(data))}


class AuthenticationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory(prefix="opennow-test-signing-")
        cls.root = Path(cls.directory.name)
        cls.openssl = shutil.which("openssl", path=host.host_environment().get("PATH"))
        if not cls.openssl:
            raise RuntimeError("Signature tests require host OpenSSL with Ed25519 support")
        subprocess.run([cls.openssl, "genpkey", "-algorithm", "Ed25519", "-out", str(cls.root / "private.pem")],
                       check=True, capture_output=True, timeout=15, env=host.host_environment())
        public = subprocess.run([cls.openssl, "pkey", "-in", str(cls.root / "private.pem"), "-pubout", "-outform", "DER"],
                                check=True, capture_output=True, timeout=15, env=host.host_environment())
        cls.public_key = public.stdout[-32:]

    @classmethod
    def tearDownClass(cls):
        cls.directory.cleanup()

    def setUp(self):
        self.work = tempfile.TemporaryDirectory()
        self.addCleanup(self.work.cleanup)
        self.selected = support.selected_asset()
        self.manifest = {"schemaVersion": 1, "version": "1.0.4", "asset": self.selected["name"], "size": 6,
                         "sha256": hashlib.sha256(b"bundle").hexdigest()}

    def sign(self, manifest):
        payload = ("OpenNOW update manifest v1\n"
                   f"version={manifest['version'].removeprefix('v')}\nasset={manifest['asset']}\n"
                   f"size={manifest['size']}\nsha256={manifest['sha256'].lower()}\n").encode()
        (self.root / "payload").write_bytes(payload)
        signature = subprocess.run([self.openssl, "pkeyutl", "-sign", "-rawin", "-inkey", str(self.root / "private.pem"),
                                    "-in", str(self.root / "payload")], check=True, capture_output=True,
                                   timeout=15, env=host.host_environment())
        return {**manifest, "signature": base64.b64encode(signature.stdout).decode()}

    def install(self, manifest_bytes, payload=b"bundle", droiddeck=False, selected=None, verifier=True):
        selected = copy.deepcopy(selected or self.selected)
        selected["manifest"]["size"] = len(manifest_bytes)

        def response(url, **kwargs):
            self.assertEqual(kwargs["hosts"], installer.DOWNLOAD_HOSTS)
            return Response(manifest_bytes if url == selected["manifest"]["url"] else payload)

        def which(command, **kwargs):
            return (self.openssl if verifier else None) if command == "openssl" else "/usr/bin/flatpak"

        def run(command, timeout):
            return subprocess.CompletedProcess(command, int(droiddeck and command[1] == "info"), "", "")

        with mock.patch.object(installer, "pinned_public_key", return_value=self.public_key), \
                mock.patch.object(installer, "open_url", side_effect=response), \
                mock.patch.object(installer.shutil, "which", side_effect=which), \
                mock.patch.object(installer.os, "access", return_value=droiddeck), \
                mock.patch.object(installer.host, "run", side_effect=run) as flatpak, \
                mock.patch.object(installer, "install_with_droiddeck") as helper:
            try:
                installer.install_bundle(selected, Path(self.work.name), lambda *a: None, threading.Event())
            except Exception:
                flatpak.assert_not_called()
                helper.assert_not_called()
                raise
            return flatpak, helper

    def test_real_signatures_authenticate_stable_and_nightly_canonical_payloads(self):
        for version in ("1.0.4", "1.0.5-nightly.1234.1", "1.0.5-nightly.1234.1+build.2"):
            selected = support.selected_asset(version)
            for prefix in ("", "v"):
                signed = self.sign({**self.manifest, "version": prefix + version, "asset": selected["name"],
                                    "sha256": self.manifest["sha256"].upper()})
                with mock.patch.object(installer, "pinned_public_key", return_value=self.public_key):
                    self.assertEqual(installer.verify_manifest(json.dumps(signed).encode(), selected), signed)

    def test_real_signature_rejects_tampering_and_the_wrong_public_key(self):
        signed = self.sign(self.manifest)
        for changed in ({**signed, "sha256": "a" * 64}, {**signed, "signature": base64.b64encode(b"x" * 64).decode()}):
            with mock.patch.object(installer, "pinned_public_key", return_value=self.public_key), \
                    self.assertRaisesRegex(installer.InstallError, "signature"):
                installer.verify_manifest(json.dumps(changed).encode(), self.selected)
        with self.assertRaisesRegex(installer.InstallError, "signature"):
            installer.verify_manifest(json.dumps(signed).encode(), self.selected)

    def test_valid_signed_bytes_reach_only_the_ordinary_flatpak_installer(self):
        flatpak, helper = self.install(json.dumps(self.sign(self.manifest)).encode())
        self.assertIn("install", [call.args[0][1] for call in flatpak.call_args_list])
        helper.assert_not_called()
        self.assertEqual(list(Path(self.work.name).iterdir()), [])

    def test_valid_signed_bytes_reach_the_droiddeck_helper(self):
        signed = json.dumps(self.sign(self.manifest)).encode()

        def run(command, timeout):
            if command[1] == "info":
                return subprocess.CompletedProcess(command, 0 if helper.called else 1, "", "")
            return subprocess.CompletedProcess(command, 0, "", "")

        self.selected["manifest"]["size"] = len(signed)
        with mock.patch.object(installer, "pinned_public_key", return_value=self.public_key), \
                mock.patch.object(installer, "open_url", side_effect=lambda url, **kw: Response(signed if url.endswith(".json") else b"bundle")), \
                mock.patch.object(installer.shutil, "which", side_effect=lambda name, **kw: self.openssl if name == "openssl" else "/usr/bin/flatpak"), \
                mock.patch.object(installer.os, "access", return_value=True), \
                mock.patch.object(installer.host, "run", side_effect=run), \
                mock.patch.object(installer, "install_with_droiddeck") as helper:
            installer.install_bundle(self.selected, Path(self.work.name), lambda *a: None, threading.Event())
        helper.assert_called_once()

    def test_wrong_metadata_or_bundle_bytes_fail_before_both_installers(self):
        signed = self.sign(self.manifest)
        invalid = [b"{}", b"not json", json.dumps({**signed, "signature": ""}).encode(),
                   json.dumps({**signed, "sha256": "a" * 64}).encode()]
        for fields in ({"version": "1.0.3"}, {"asset": "OpenNOW-Qt-1.0.4-Linux-arm64.flatpak"},
                       {"size": 7}, {"schemaVersion": 2}):
            invalid.append(json.dumps(self.sign({**self.manifest, **fields})).encode())
        for droiddeck in (False, True):
            for data in invalid:
                with self.subTest(droiddeck=droiddeck, manifest=data), self.assertRaises(installer.InstallError):
                    self.install(data, droiddeck=droiddeck)
            for payload in (b"tamper", b"bundl", b"bundle!"):
                with self.subTest(droiddeck=droiddeck, payload=payload), self.assertRaises(installer.InstallError):
                    self.install(json.dumps(signed).encode(), payload=payload, droiddeck=droiddeck)
        self.assertEqual(list(Path(self.work.name).iterdir()), [])

    def test_missing_or_inconsistent_selection_never_contacts_installer_or_network(self):
        for fields in ({"manifest": None}, {"tag": "v1.0.3"}, {"architecture": "arm64"}, {"size": 0},
                       {"url": "https://github.com/unrelated"}):
            selected = {**self.selected, **fields}
            with mock.patch.object(installer, "open_url") as network, \
                    mock.patch.object(installer.host, "run") as flatpak, \
                    mock.patch.object(installer, "install_with_droiddeck") as helper, \
                    self.assertRaises(installer.InstallError):
                installer.install_bundle(selected, Path(self.work.name), lambda *a: None, threading.Event())
            network.assert_not_called()
            flatpak.assert_not_called()
            helper.assert_not_called()

    def test_missing_or_stalled_host_verifier_fails_closed(self):
        data = json.dumps(self.sign(self.manifest)).encode()
        for droiddeck in (False, True):
            with self.subTest(droiddeck=droiddeck), self.assertRaisesRegex(installer.InstallError, "OpenSSL"):
                self.install(data, droiddeck=droiddeck, verifier=False)
        with mock.patch.object(installer.shutil, "which", return_value=None), \
                self.assertRaisesRegex(installer.InstallError, "OpenSSL"):
            installer.verify_manifest(data, self.selected)
        with mock.patch.object(installer.subprocess, "run", side_effect=subprocess.TimeoutExpired("openssl", 15)), \
                self.assertRaisesRegex(installer.InstallError, "OpenSSL"):
            installer.verify_manifest(data, self.selected)

    def test_manifest_is_bounded_even_without_content_length(self):
        self.selected["manifest"]["size"] = installer.MAX_MANIFEST_BYTES
        response = Response(b"x" * (installer.MAX_MANIFEST_BYTES + 1))
        response.headers = {}
        with mock.patch.object(installer, "open_url", return_value=response), \
                self.assertRaises(installer.InstallError):
            installer.signed_manifest(self.selected, threading.Event())


class RedirectTests(unittest.TestCase):
    def test_every_redirect_enforces_https_and_host_allowlist(self):
        handler = net.AllowedRedirects(installer.DOWNLOAD_HOSTS)
        request = urllib.request.Request("https://github.com/start")
        for url in ("http://github.com/asset", "https://attacker.invalid/asset", "https://github.com.attacker.invalid/asset",
                    "https://user@github.com/asset", "https://github.com:444/asset"):
            with self.subTest(url=url), self.assertRaises(net.HostError):
                handler.redirect_request(request, None, 302, "Found", {}, url)
        redirect = handler.redirect_request(request, None, 302, "Found", {}, "https://release-assets.githubusercontent.com/asset")
        self.assertEqual(redirect.full_url, "https://release-assets.githubusercontent.com/asset")

    def test_authenticated_downloads_use_the_restricted_redirect_handler(self):
        with mock.patch.object(net.urllib.request, "build_opener") as opener:
            net.open_url("https://github.com/start", hosts=installer.DOWNLOAD_HOSTS)
        self.assertIsInstance(opener.call_args.args[0], net.AllowedRedirects)
        self.assertEqual(opener.call_args.args[0].hosts, installer.DOWNLOAD_HOSTS)
