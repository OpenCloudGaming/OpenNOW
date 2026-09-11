import importlib.util
from pathlib import Path
import stat
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github/scripts/prepare-ubuntu-apt-https.py"
SPEC = importlib.util.spec_from_file_location("ubuntu_apt_https", SCRIPT)
APT_HTTPS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(APT_HTTPS)


class UbuntuAptHttpsTest(unittest.TestCase):
    def test_list_preserves_suites_components_options_and_comments(self):
        source = (
            "# http://archive.ubuntu.com/ubuntu is a documentation example\r\n"
            "deb [arch=amd64 signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg] "
            "http://archive.ubuntu.com/ubuntu/ noble main universe restricted multiverse # keep\r\n"
            "deb-src http://security.ubuntu.com/ubuntu noble-security main\r\n"
            "deb mirror+file:/etc/apt/blacksmith-ubuntu-mirrors.txt noble-updates main\r\n"
        )
        expected = source.replace("http://archive.ubuntu.com/ubuntu/ noble", "https://archive.ubuntu.com/ubuntu/ noble")
        expected = expected.replace("deb-src http://security", "deb-src https://security")
        self.assertEqual(APT_HTTPS.rewrite(source, "list"), expected)

    def test_deb822_updates_only_uri_fields_including_continuations(self):
        source = (
            "Types: deb deb-src\n"
            "URIs: http://archive.ubuntu.com/ubuntu https://example.org/other\n"
            " http://security.ubuntu.com/ubuntu/\n"
            "Suites: noble noble-updates noble-backports noble-security\n"
            "Components: main universe restricted multiverse\n"
            "Architectures: amd64\n"
            "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n"
            "X-Comment: http://archive.ubuntu.com/ubuntu\n\n"
            "Types: deb\n"
            "URIs: mirror+file:/etc/apt/blacksmith-ubuntu-mirrors.txt\n"
            "Suites: noble\n"
            "Components: main\n"
            "Signed-By:\n -----BEGIN PGP PUBLIC KEY BLOCK-----\n .\n test-key-data\n -----END PGP PUBLIC KEY BLOCK-----\n"
        )
        expected = source.replace("URIs: http://archive", "URIs: https://archive")
        expected = expected.replace(" http://security", " https://security")
        self.assertEqual(APT_HTTPS.rewrite(source, "sources"), expected)

    def test_blacksmith_mirror_file_preserves_priorities_and_other_repositories(self):
        source = (
            "# Ubuntu mirrors\nhttp://archive.ubuntu.com/ubuntu/\n"
            "http://mirrors.sonic.net/ubuntu\tpriority:1\n"
            "http://us.archive.ubuntu.com/ubuntu priority:2\n"
            "http://example.org/debian\n"
        )
        expected = source.replace("http://archive.", "https://archive.")
        expected = expected.replace("http://mirrors.sonic.net", "https://archive.ubuntu.com")
        expected = expected.replace("http://us.archive.", "https://us.archive.")
        self.assertEqual(APT_HTTPS.rewrite(source, "mirrors"), expected)

    def test_sonic_http_ubuntu_mirror_maps_to_official_archive(self):
        for suffix in ("", "/"):
            with self.subTest(suffix=suffix):
                source = f"http://mirrors.sonic.net/ubuntu{suffix}\tpriority:1\n"
                expected = f"https://archive.ubuntu.com/ubuntu{suffix}\tpriority:1\n"
                self.assertEqual(APT_HTTPS.rewrite(source, "mirrors"), expected)
        self.assertEqual(APT_HTTPS.https_uri("https://mirrors.sonic.net/other"), "https://mirrors.sonic.net/other")

    def test_unverified_hosts_nonubuntu_paths_and_existing_https_are_unchanged(self):
        for uri in (
            "http://packages.microsoft.com/ubuntu/24.04/prod",
            "http://ppa.launchpad.net/provider/tool/ubuntu",
            "http://ports.ubuntu.com/ubuntu-ports",
            "http://example.org/ubuntu",
            "http://archive.ubuntu.com.evil.example/ubuntu",
            "http://archive.ubuntu.com/other",
            "http://archive.ubuntu.com:80/ubuntu",
            "https://archive.ubuntu.com/ubuntu",
            "https://mirrors.sonic.net/ubuntu",
            "mirror+file:/etc/apt/blacksmith-ubuntu-mirrors.txt",
        ):
            with self.subTest(uri=uri):
                self.assertEqual(APT_HTTPS.https_uri(uri), uri)

    def test_prepare_changes_only_selected_files_and_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            apt = Path(directory)
            sources = apt / "sources.list.d"
            sources.mkdir()
            files = {
                apt / "sources.list": "deb http://archive.ubuntu.com/ubuntu noble main\n",
                sources / "ubuntu.sources": "Types: deb\nURIs: http://security.ubuntu.com/ubuntu\nSuites: noble-security\nComponents: main\n",
                sources / "extra.list": "deb-src http://us.archive.ubuntu.com/ubuntu noble main\n",
                apt / "blacksmith-ubuntu-mirrors.txt": "http://mirrors.sonic.net/ubuntu\n",
                sources / "vendor.list": "deb http://vendor.example/ubuntu noble main\n",
                sources / "ubuntu.list.save": "deb http://archive.ubuntu.com/ubuntu noble main\n",
                apt / "apt.conf": 'Acquire::https::Verify-Peer "true";\n',
            }
            for path, text in files.items():
                path.write_text(text)
                path.chmod(0o640)
            originals = {path: path.stat() for path in files}
            changed = set(APT_HTTPS.prepare(apt))
            self.assertEqual(changed, {apt / "sources.list", sources / "ubuntu.sources",
                                       sources / "extra.list", apt / "blacksmith-ubuntu-mirrors.txt"})
            for path, text in files.items():
                expected = text.replace("http://", "https://").replace("mirrors.sonic.net", "archive.ubuntu.com") if path in changed else text
                self.assertEqual(path.read_text(), expected)
                self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o640)
                self.assertEqual((path.stat().st_uid, path.stat().st_gid),
                                 (originals[path].st_uid, originals[path].st_gid))
                if path not in changed:
                    self.assertEqual(path.stat().st_mtime_ns, originals[path].st_mtime_ns)
            modified = {path: path.stat().st_mtime_ns for path in files}
            self.assertEqual(APT_HTTPS.prepare(apt), [])
            self.assertEqual({path: path.stat().st_mtime_ns for path in files}, modified)
            self.assertEqual({path for path in apt.rglob("*") if path.is_file()}, set(files))

    def test_preparation_refuses_to_replace_a_linked_source(self):
        with tempfile.TemporaryDirectory() as directory:
            apt = Path(directory)
            original = apt / "original"
            source = "deb http://archive.ubuntu.com/ubuntu noble main\n"
            original.write_text(source)
            (apt / "sources.list").symlink_to(original)
            with self.assertRaisesRegex(ValueError, "linked apt source"):
                APT_HTTPS.prepare(apt)
            self.assertEqual(original.read_text(), source)

    def test_preparation_runs_before_any_apt_or_qt_install_only_on_linux_x64(self):
        command = "sudo python3 .github/scripts/prepare-ubuntu-apt-https.py"
        for path, condition in (
            (ROOT / ".github/workflows/qt-build.yml", "if: matrix.label == 'linux-x64'"),
            (ROOT / ".github/actions/qt-unit-tests/action.yml", "if: inputs.label == 'linux-x64'"),
        ):
            with self.subTest(path=path):
                text = path.read_text()
                self.assertEqual(text.count(command), 1)
                setup = text.split("name: Prepare Ubuntu HTTPS mirrors", 1)[1].split("run: " + command, 1)[0]
                self.assertIn(condition, setup)
                self.assertLess(text.index(command), text.index("apt-get"))
                self.assertLess(text.index(command), text.index("uses: jurplel/install-qt-action@v4"))


if __name__ == "__main__":
    unittest.main()
