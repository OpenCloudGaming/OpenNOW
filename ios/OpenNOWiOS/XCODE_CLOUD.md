# TestFlight on every iOS push

Use the existing OpenNOW Xcode Cloud product connected to `OpenCloudGaming/OpenNOW`. Apple manages signing and uploads, so GitHub signing secrets and an App Store Connect API key are unnecessary.

Workflow configuration:

| Setting | Value |
| --- | --- |
| Name | iOS TestFlight |
| Repository | `OpenCloudGaming/OpenNOW` |
| Project | `ios/OpenNOWiOS/OpenNOWiOS.xcodeproj` |
| Start condition | Branch Changes → Custom Branches → `kief5555/ios` |
| Files and folders | Start a Build If Any File Changes |
| Auto-cancel builds | Off, so another push does not cancel a queued/running build |
| Environment | Latest Release Xcode 27 and compatible macOS |
| Required action | Archive → iOS → scheme `OpenNOWiOS` → App Store Connect |
| Optional post-action | TestFlight Internal Testing → the existing internal tester group |

The Archive action signs and uploads the app to App Store Connect for TestFlight processing. The post-action distributes successful builds to a selected tester group. External groups may require Beta App Review; a public App Store release still requires submission for review.

Xcode Cloud automatically increments its build number for each new build. The next build number was set to **191** during setup, above the source build 190 and the existing TestFlight uploads. To change it later, use **Product Settings → Build Number → Next Build Number**. The app and widget both use `$(CURRENT_PROJECT_VERSION)`. Marketing version remains the checked-in `MARKETING_VERSION`; no version-only commits or extra push loops are needed.

`ci_scripts/ci_post_clone.sh` runs the existing checksum-pinned native library preparation script before Xcode builds. Keep it executable and beside `OpenNOWiOS.xcodeproj`. Without this step, a clean cloud checkout is missing the ignored OpenSSL and usrsctp XCFrameworks.

Save and push the script on `kief5555/ios` before starting the first cloud build. Check that the resulting cloud run lists this branch and the pushed commit, completes its archive/upload, and appears in TestFlight after Apple processing. A successful unsigned local archive verifies compilation and matched versions, but does not prove Apple signing or upload acceptance.

If App Store Connect says an updated Apple Developer Program agreement is required, the Account Holder must review and accept it. The older OpenNOW cloud runs targeted `main`; use this iOS-only workflow to avoid trying to build the iOS project from the desktop branch. If the web editor reports no shared schemes, configure the workflow from Xcode with this project open using **Integrate → Create/Manage Workflows** and select its shared `OpenNOWiOS` scheme.

References: [Apple workflow setup](https://developer.apple.com/documentation/xcode/configuring-your-first-xcode-cloud-workflow), [distribution workflows](https://developer.apple.com/documentation/xcode/creating-a-workflow-that-builds-your-app-for-distribution), [custom build scripts](https://developer.apple.com/documentation/xcode/writing-custom-build-scripts).
