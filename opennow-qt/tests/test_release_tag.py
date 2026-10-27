from pathlib import Path
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "opennow-qt/packaging"))
from verify_release_tag import verify_tag


class ReleaseTagTest(unittest.TestCase):
    repository = "OpenCloudGaming/OpenNOW"
    tag = "v1.2.3"
    commit = "a" * 40

    def reference(self, kind="commit", sha=None, tag=None):
        return {"ref": "refs/tags/" + (tag or self.tag),
                "object": {"type": kind, "sha": sha or self.commit}}

    def verify(self, responses, allow_missing=False, create_missing=False):
        with patch("verify_release_tag.subprocess.run") as run:
            run.side_effect = [subprocess.CompletedProcess([], 0, json.dumps(value))
                               for value in responses]
            verify_tag(self.repository, self.tag, self.commit, allow_missing, create_missing)
            return run

    def test_explicitly_creates_only_a_missing_tag_at_the_verified_commit(self):
        run = self.verify([[], self.reference()], create_missing=True)
        self.assertEqual(run.call_args.args[0],
                         ["gh", "api", f"repos/{self.repository}/git/refs", "--method", "POST",
                          "-f", "ref=refs/tags/" + self.tag, "-f", "sha=" + self.commit])
        run = self.verify([[self.reference()]], create_missing=True)
        self.assertEqual(run.call_count, 1)
        with self.assertRaisesRegex(ValueError, "does not match"):
            self.verify([[self.reference(sha="b" * 40)]], create_missing=True)
        for created in ({}, [], self.reference(tag="v1.2.30"), self.reference(sha="b" * 40)):
            with self.subTest(created=created):
                with self.assertRaises(ValueError):
                    self.verify([[], created], create_missing=True)

    def test_absent_tag_is_allowed_only_before_creation(self):
        self.verify([[]], allow_missing=True)
        with self.assertRaisesRegex(ValueError, "does not exist"):
            self.verify([[]])
        with self.assertRaisesRegex(ValueError, "does not exist"):
            self.verify([[self.reference(tag="v1.2.30")]])

    def test_matching_lightweight_and_nested_annotated_tags(self):
        self.verify([[self.reference()]])
        run = self.verify([[self.reference("tag", "b" * 40)],
                           {"object": {"type": "tag", "sha": "c" * 40}},
                           {"object": {"type": "commit", "sha": self.commit}}])
        self.assertEqual(run.call_args_list[1].args[0],
                         ["gh", "api", f"repos/{self.repository}/git/tags/" + "b" * 40])
        self.assertEqual(run.call_args.kwargs["timeout"], 30)

    def test_wrong_commit_never_passes_even_before_creation(self):
        for allow_missing in (False, True):
            for responses in ([[self.reference(sha="b" * 40)]],
                              [[self.reference("tag", "b" * 40)],
                               {"object": {"type": "commit", "sha": "c" * 40}}]):
                with self.subTest(allow_missing=allow_missing, responses=responses):
                    with self.assertRaisesRegex(ValueError, "does not match"):
                        self.verify(responses, allow_missing)

    def test_invalid_targets_and_excessive_nesting_fail_closed(self):
        for responses in ([{}], [[None]], [[self.reference(), self.reference()]],
                          [[{"ref": "refs/tags/" + self.tag}]],
                          [[self.reference(sha="invalid")]], [[self.reference("tree")]],
                          [[self.reference("tag")], {}],
                          [[self.reference("tag")]] + [{"object": self.reference("tag")["object"]}] * 16):
            with self.subTest(responses=responses):
                with self.assertRaises(ValueError):
                    self.verify(responses)

    def test_github_failures_are_not_treated_as_absent_tags(self):
        for failure in (OSError("missing gh"), subprocess.TimeoutExpired("gh", 30),
                        subprocess.CalledProcessError(1, "gh")):
            with self.subTest(failure=failure):
                with patch("verify_release_tag.subprocess.run", side_effect=failure):
                    with self.assertRaisesRegex(ValueError, "Could not read"):
                        verify_tag(self.repository, self.tag, self.commit, True)

    def test_tag_creation_failures_identify_the_failed_write(self):
        with patch("verify_release_tag.subprocess.run", side_effect=[
                subprocess.CompletedProcess([], 0, "[]"),
                subprocess.CalledProcessError(1, "gh")]):
            with self.assertRaisesRegex(ValueError, "Could not create release tag"):
                verify_tag(self.repository, self.tag, self.commit, create_missing=True)

    def test_invalid_arguments_do_not_reach_github(self):
        for repository, tag, commit in (("owner/repo/extra", self.tag, self.commit),
                                        (self.repository, "refs/tags/v1.2.3", self.commit),
                                        (self.repository, self.tag, "main")):
            with patch("verify_release_tag.subprocess.run") as run:
                with self.assertRaises(ValueError):
                    verify_tag(repository, tag, commit)
                run.assert_not_called()


class ReleaseTagWorkflowTest(unittest.TestCase):
    def test_tag_identity_is_checked_before_creating_and_publishing_releases(self):
        for name in ("qt-stable-release.yml", "qt-ci.yml"):
            with self.subTest(workflow=name):
                workflow = (ROOT / ".github/workflows" / name).read_text()
                before_create, remaining = workflow.split("gh release create", 1)
                before_publish = remaining.split("gh release edit", 1)[0]
                self.assertIn("verify_release_tag.py", before_create)
                self.assertIn("--allow-missing", before_create)
                self.assertIn("verify_release_tag.py", before_publish)
                self.assertIn("--create-missing", before_publish)
                self.assertNotIn("--allow-missing", before_publish)

    def test_wrong_or_moved_tags_cannot_publish_a_release(self):
        for name, steps in (("qt-stable-release.yml", ("Upload as draft before publishing the complete release",)),
                            ("qt-ci.yml", ("Upload the complete draft release", "Publish the verified draft"))):
            workflow = (ROOT / ".github/workflows" / name).read_text()
            scripts = []
            for step in steps:
                entry = workflow.split(f"      - name: {step}\n", 1)[1].split("      - name:", 1)[0]
                scripts.append(textwrap.dedent(entry.split("        run: |\n", 1)[1]))
            for existing, move, succeeds in ((None, False, True), ("a" * 40, False, True),
                                             ("b" * 40, False, False), (None, True, False)):
                with self.subTest(workflow=name, existing=existing, move=move):
                    with tempfile.TemporaryDirectory() as directory:
                        root = Path(directory)
                        (root / "bin").mkdir()
                        (root / "release").mkdir()
                        (root / "release/fixture").write_bytes(b"fixture")
                        (root / "nightly-release").mkdir()
                        (root / "nightly-release/fixture").write_bytes(b"fixture")
                        (root / "opennow-qt/packaging").mkdir(parents=True)
                        (root / "opennow-qt/packaging/verify_release_tag.py").symlink_to(
                            ROOT / "opennow-qt/packaging/verify_release_tag.py")
                        state = root / "state.json"
                        state.write_text(json.dumps({"tag": existing, "move": move, "draft": None}))
                        gh = root / "bin/gh"
                        gh.write_text("#!/usr/bin/env python3\n" + textwrap.dedent('''
                            import json
                            import os
                            from pathlib import Path
                            import sys
                            path = Path(os.environ["GH_FIXTURE_STATE"])
                            state = json.loads(path.read_text())
                            args = sys.argv[1:]
                            if args[0] == "api":
                                if "--method" in args:
                                    assert args[1].endswith("/git/refs"), args
                                    assert args[args.index("--method") + 1] == "POST", args
                                    assert state["tag"] is None
                                    state["tag"] = next(arg[4:] for arg in args if arg.startswith("sha="))
                                    print(json.dumps({"ref": "refs/tags/v1.2.3",
                                          "object": {"type": "commit", "sha": state["tag"]}}))
                                else:
                                    assert args[1].endswith("/git/matching-refs/tags/v1.2.3"), args
                                    print(json.dumps([] if state["tag"] is None else [{
                                        "ref": "refs/tags/v1.2.3",
                                        "object": {"type": "commit", "sha": state["tag"]}}]))
                            elif args[:2] == ["release", "create"]:
                                state["target"] = args[args.index("--target") + 1]
                                state["draft"] = True
                                if state["move"]:
                                    state["tag"] = "b" * 40
                            elif args[:2] == ["release", "edit"]:
                                state["tag"] = state["tag"] or state["target"]
                                state["draft"] = False
                            else:
                                raise SystemExit("Unexpected fixture command")
                            path.write_text(json.dumps(state))
                        '''))
                        gh.chmod(0o700)
                        result = subprocess.run(["bash", "-euo", "pipefail", "-c", "\n".join(scripts)],
                                                cwd=root, capture_output=True, text=True,
                                                env={**os.environ, "PATH": str(root / "bin") + os.pathsep + os.environ["PATH"],
                                                     "GH_FIXTURE_STATE": str(state), "RELEASE_VERSION": "1.2.3",
                                                     "SOURCE_SHA": "a" * 40, "GITHUB_REPOSITORY": "fixture/fixture"})
                        self.assertEqual(result.returncode == 0, succeeds, result.stderr)
                        published = json.loads(state.read_text())
                        self.assertEqual(published["draft"] is False, succeeds)
                        if succeeds:
                            self.assertEqual(published["tag"], "a" * 40)


if __name__ == "__main__":
    unittest.main()
