import argparse
import json
import re
import subprocess


def github(path, *arguments, failure="Could not read release tag identity from GitHub"):
    try:
        response = subprocess.run(["gh", "api", path, *arguments], check=True, capture_output=True,
                                  text=True, timeout=30)
        return json.loads(response.stdout)
    except (OSError, subprocess.SubprocessError, ValueError):
        raise ValueError(failure) from None


def verify_tag(repository, tag, commit, allow_missing=False, create_missing=False):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
        raise ValueError("Expected an owner/repository identity")
    if not re.fullmatch(r"v[0-9][A-Za-z0-9.+-]*", tag):
        raise ValueError("Expected a version release tag")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("Expected an exact source commit")
    references = github(f"repos/{repository}/git/matching-refs/tags/{tag}")
    if not isinstance(references, list) or any(not isinstance(ref, dict) for ref in references):
        raise ValueError("GitHub returned invalid tag references")
    matches = [ref for ref in references if ref.get("ref") == f"refs/tags/{tag}"]
    if not matches and create_missing:
        created = github(f"repos/{repository}/git/refs", "--method", "POST",
                         "-f", f"ref=refs/tags/{tag}", "-f", f"sha={commit}",
                         failure="Could not create release tag at the verified source commit on GitHub")
        if not isinstance(created, dict) or created.get("ref") != f"refs/tags/{tag}":
            raise ValueError("GitHub returned an invalid created tag reference")
        matches = [created]
    if not matches:
        if allow_missing:
            return
        raise ValueError("The release tag does not exist")
    if len(matches) != 1:
        raise ValueError("GitHub returned ambiguous tag references")
    target = matches[0].get("object")
    for _ in range(16):
        if (not isinstance(target, dict) or not isinstance(target.get("sha"), str)
                or not re.fullmatch(r"[0-9a-f]{40}", target["sha"])):
            raise ValueError("GitHub returned an invalid tag target")
        if target.get("type") == "commit":
            if target["sha"] != commit:
                raise ValueError("Release tag does not match the verified source commit")
            return
        if target.get("type") != "tag":
            raise ValueError("Release tag does not resolve to a commit")
        annotated = github(f"repos/{repository}/git/tags/{target['sha']}")
        target = annotated.get("object") if isinstance(annotated, dict) else None
    raise ValueError("Release tag nesting exceeds the verification limit")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--commit", required=True)
    missing = parser.add_mutually_exclusive_group()
    missing.add_argument("--allow-missing", action="store_true")
    missing.add_argument("--create-missing", action="store_true")
    args = parser.parse_args()
    try:
        verify_tag(args.repository, args.tag, args.commit, args.allow_missing, args.create_missing)
    except ValueError as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
