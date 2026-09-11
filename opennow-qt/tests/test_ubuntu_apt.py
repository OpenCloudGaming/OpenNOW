from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / ".github/scripts/configure-ubuntu-apt.sh"


class UbuntuAptTest(unittest.TestCase):
    def test_ubuntu_sources_use_https_without_changing_trust_or_suites(self):
        for uri in ("http://archive.ubuntu.com/ubuntu/",
                    "http://us.archive.ubuntu.com/ubuntu/",
                    "mirror+file:/etc/apt/blacksmith-ubuntu-mirrors.txt"):
            with self.subTest(uri=uri), tempfile.TemporaryDirectory() as directory:
                apt = Path(directory)
                sources = apt / "sources.list.d/ubuntu.sources"
                sources.parent.mkdir()
                original = (
                    f"Types: deb\nURIs: {uri}\n"
                    "Suites: noble noble-updates noble-backports\n"
                    "Components: main restricted universe multiverse\n"
                    "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n\n"
                    "Types: deb\nURIs: http://security.ubuntu.com/ubuntu/\n"
                    "Suites: noble-security\nComponents: main restricted universe multiverse\n"
                    "Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n"
                )
                sources.write_text(original)
                mirrors = apt / "blacksmith-ubuntu-mirrors.txt"
                if uri.startswith("mirror+"):
                    mirrors.write_text("http://archive.ubuntu.com/ubuntu\nhttp://mirrors.sonic.net/ubuntu\n")
                other = sources.parent / "vendor.sources"
                other.write_text("URIs: https://packages.microsoft.com/ubuntu/24.04/prod\n")

                subprocess.run(["bash", str(SCRIPT), str(apt)], check=True)

                expected = original.replace("http://security.ubuntu.com", "https://security.ubuntu.com")
                if uri.startswith("http:"):
                    expected = expected.replace(uri, "https://archive.ubuntu.com/ubuntu/")
                    self.assertFalse(mirrors.exists())
                else:
                    self.assertEqual(mirrors.read_text(), "https://archive.ubuntu.com/ubuntu\n")
                self.assertEqual(sources.read_text(), expected)
                self.assertEqual(other.read_text(), "URIs: https://packages.microsoft.com/ubuntu/24.04/prod\n")
                subprocess.run(["bash", str(SCRIPT), str(apt)], check=True)
                self.assertEqual(sources.read_text(), expected)

    def test_missing_ubuntu_sources_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(["bash", str(SCRIPT), directory])
            self.assertNotEqual(result.returncode, 0)

    def test_ci_configures_sources_before_the_first_apt_update(self):
        action = (ROOT / ".github/actions/qt-unit-tests/action.yml").read_text()
        self.assertLess(action.index("sudo bash .github/scripts/configure-ubuntu-apt.sh"),
                        action.index("sudo apt-get update"))
