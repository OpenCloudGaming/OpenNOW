import importlib.util
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "update_sources", ROOT / "meta-opennow/scripts/update-sources.py"
)
SOURCES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SOURCES)
REVISION = "8c812c4a0087eaf664a11a844634200cebe9afb8"
SDL_SOURCE = f"git+https://github.com/zortos293/rust-sdl2.git?rev={REVISION}#{REVISION}"


class SourceMetadataTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.write_lock("opennow-core", self.registry_package())
        self.write_lock("opennow-streamer", self.registry_package() + self.sdl_packages())

    def write_lock(self, workspace, contents):
        directory = self.root / "native" / workspace
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "Cargo.lock").write_text("version = 4\n" + contents)

    def registry_package(self, checksum="a" * 64):
        return (
            '[[package]]\nname = "shared"\nversion = "1.2.3"\n'
            'source = "registry+https://github.com/rust-lang/crates.io-index"\n'
            f'checksum = "{checksum}"\n'
        )

    def sdl_packages(self):
        return "".join(
            f'[[package]]\nname = "{name}"\nversion = "0.38.0"\nsource = "{SDL_SOURCE}"\n'
            for name in ("sdl2", "sdl2-sys")
        )

    def test_shared_crates_are_fetched_once_with_locked_checksum(self):
        outputs = SOURCES.render_crates(self.root)
        crates = outputs["opennow-crates.inc"]
        self.assertEqual(crates.count("crate://crates.io/shared/1.2.3"), 1)
        self.assertIn(f'SRC_URI[shared-1.2.3.sha256sum] = "{"a" * 64}"', crates)
        self.assertIn(f'SRCREV_sdl2 = "{REVISION}"', outputs["opennow-sdl.inc"])
        self.assertIn("gitsm://", outputs["opennow-sdl.inc"])

    def test_conflicting_checksums_are_rejected(self):
        self.write_lock("opennow-core", self.registry_package("b" * 64))
        with self.assertRaisesRegex(ValueError, "Conflicting checksums"):
            SOURCES.render_crates(self.root)

    def test_distinct_versions_are_preserved(self):
        self.write_lock("opennow-core", self.registry_package().replace("1.2.3", "2.0.0"))
        crates = SOURCES.render_crates(self.root)["opennow-crates.inc"]
        self.assertIn("crate://crates.io/shared/1.2.3", crates)
        self.assertIn("crate://crates.io/shared/2.0.0", crates)

    def test_mismatched_sdl_revisions_are_rejected(self):
        packages = self.sdl_packages().replace(f'#{REVISION}', '#' + 'b' * 40, 1)
        self.write_lock("opennow-streamer", packages)
        with self.assertRaisesRegex(ValueError, "must share one source revision"):
            SOURCES.render_crates(self.root)

    def test_unknown_git_dependencies_are_rejected(self):
        self.write_lock("opennow-core", self.registry_package() +
                        '[[package]]\nname = "new-git"\nversion = "1.0.0"\n'
                        'source = "git+https://example.com/new-git#abc"\n')
        with self.assertRaisesRegex(ValueError, "Unexpected Git packages"):
            SOURCES.render_crates(self.root)

    def test_unknown_registry_is_rejected(self):
        self.write_lock("opennow-core", self.registry_package().replace(
            "registry+https://github.com/rust-lang/crates.io-index", "registry+https://example.com"
        ))
        with self.assertRaisesRegex(ValueError, "Unsupported Cargo source"):
            SOURCES.render_crates(self.root)

    def test_generated_repository_metadata_is_current(self):
        for name, expected in SOURCES.render_crates(ROOT).items():
            self.assertEqual((ROOT / "meta-opennow/recipes-games/opennow" / name).read_text(), expected)


if __name__ == "__main__":
    unittest.main()
