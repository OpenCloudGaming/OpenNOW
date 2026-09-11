# Activate nightly update signing

Publication is blocked until a repository administrator configures signing. At the
time of this implementation, the `qt-update-signing` GitHub environment does not exist,
and the available API credentials return HTTP 403 for its configuration. The workflow
change does not create an environment, provision a runner, or install a production key.

## Configure the protected signer

1. In the repository's Settings → Environments, create `qt-update-signing`.
2. Enable required reviewers and prevent self-review. Disable administrator bypass
   where the repository plan permits it.
3. Restrict deployment branches and tags to the protected release refs your reviewers
   approve. For nightly dispatches from `dev`, explicitly allow protected `dev`.
   Do not allow unreviewed feature branches or pull-request refs.
4. Provision a dedicated Linux runner with both `self-hosted` and
   `opennow-release-signer` labels. Restrict its runner group to approved release
   workflows in this repository. Do not assign these labels to platform build workers.
5. Install Python 3.11 or newer and OpenSSL 3 on that runner. Reset the runner after
   every signing job, including failures and cancellations, before accepting another job.
   Do not run pull-request jobs or candidate programs on it.
6. Generate and retain a production Ed25519 key outside CI. Add only its canonical
   base64-encoded 32-byte private seed as the environment secret
   `OPENNOW_UPDATE_ED25519_PRIVATE_KEY`. Do not put the seed in repository secrets,
   workflow inputs, CMake arguments, build artifacts, logs, or a developer message.
7. Record the matching canonical base64-encoded 32-byte public key for dispatches.
   The public key is not secret. The signing job derives it independently from the
   seed and rejects a mismatch.

Environment protections are part of the trust boundary. GitHub can create an environment
name referenced by a workflow without adding protections. Verify the reviewer and ref
rules before the first dispatch; the YAML cannot enforce those server-side settings.
Reviewers must approve the exact dispatched source/workflow revision and inspect its
workflow and packaging/signing scripts before allowing access to the seed. The signer
checks out only that immutable revision, with persisted Git credentials disabled, and
treats downloaded packages as data. Never execute downloaded artifacts or candidate
programs on the signer. Only the approved release tooling may run there.

## Publish the first update-enabled nightly

1. Select the reviewed protected revision in GitHub Actions → qt-ci → Run workflow.
2. Set `public_key` to the recorded public key and set `publish_nightly` to `true`.
3. Wait for shared checks, all platform checks, and the complete package build to pass.
4. Inspect the source commit and the nine-package inventory before approving the
   `qt-update-signing` deployment.
5. Confirm that the publisher verifies the signed set and uploads all 20 files before
   making the draft prerelease public. These are nine packages, nine sibling manifests,
   `RELEASE-INFO.json`, and `SHA256SUMS`.

For artifact-only testing, leave `publish_nightly` false. An empty `public_key` is
allowed only for that non-public path. The resulting build cannot download updates
because its signature policy remains `unconfigured-fail-closed`.

Users running an earlier nightly without a pinned key must manually download and
install the first update-enabled nightly once. That older application cannot securely
learn a trust key from release metadata. Do not bypass signature checks to bootstrap it.
Later updates require manifests signed by the already pinned key.

Update-manifest signing does not remove Windows publisher warnings or macOS Gatekeeper
warnings. The platform packages remain unsigned. Keep the Alliance Partners release
warning: **Known issue: Alliance Partners are not working correctly in this build.**
Update signing does not fix partner authentication or streaming compatibility.

## Verify the repository contract without production credentials

Run the packaging and workflow tests:

```sh
python3 -m unittest discover -s opennow-qt/tests -p 'test_*.py'
actionlint -color=false .github/workflows/qt-ci.yml .github/workflows/qt-checks.yml \
  .github/workflows/qt-build.yml .github/workflows/qt-release-candidate.yml
```

The signing tests generate ephemeral test-only keys, sign and verify all nine fixture
packages with OpenSSL, and reject mismatched keys, incomplete inventories, changed
packages, changed manifests, and invalid public inputs. No production seed is needed.

The platform-check action also runs `opennow-qt/tests/run_update_helper_integration.py`
with `python` on Windows x64 and `python3` on Linux x64 and macOS ARM64. That driver
builds the helper and candidate fixtures in a temporary Cargo target directory and
runs all four signed-helper integration tests with an ephemeral key. It runs only
on platform test workers, never on the protected release signer.
