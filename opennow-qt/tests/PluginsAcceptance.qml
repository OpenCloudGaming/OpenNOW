import QtQuick
import OpenNOW

QtObject {
    property int key: 0
    property int modifiers: 0
    property int phase: 0
    property Item root: null
    property var page: null
    property var screen: null
    readonly property var store: ShellStore.pluginOwnerState
    readonly property string gfnId: "org.opennow.geforce-now"
    readonly property string exampleId: "org.opennow.example.catalog"
    readonly property bool showcase: Qt.application.arguments.indexOf("--plugins-showcase") >= 0
    readonly property string showcaseStage: showcase
        ? String(Qt.application.arguments[Qt.application.arguments.indexOf("--plugins-showcase") + 1] || "preview") : ""
    readonly property bool unavailableCheck: Qt.application.arguments.indexOf("--plugins-unavailable") >= 0
    property int startGeneration: -1
    property int settleTicks: 0

    function check(ok, message) { if (!ok) throw new Error("Plugins: " + message) }
    function find(item, name, visited) {
        const seen = visited || new Set()
        if (!item || seen.has(item))
            return null
        seen.add(item)
        if (item.objectName === name)
            return item
        for (const child of (item.children || []).concat(item.data || [])) {
            const found = find(child, name, seen)
            if (found)
                return found
        }
        return null
    }
    function example() { return store.pluginById(exampleId) }

    function run(parent) {
        root = parent
        if (Qt.application.arguments.indexOf("--plugins-signed-in") >= 0)
            ShellStore.authSession = {user: {userId: "fixture-user", displayName: "Fixture"},
                provider: {idpId: "nvidia-fixture", code: "NVIDIA", displayName: "NVIDIA"}}
        return true
    }

    function locateSurface() {
        page = find(root, "desktopPluginsSettings")
        screen = find(root, "consoleSettingsScreen")
        return page !== null || screen !== null
    }

    function advance() {
        try {
            return step()
        } catch (error) {
            console.error(error.message)
            return 2
        }
    }

    function step() {
        if (phase === 0 && showcaseStage === "signin") {
            const manage = find(root, "signInManagePlugins") || find(root, "consoleSignInManagePlugins")
            if (!manage || !manage.visible || !store.loaded)
                return 0
            settleTicks += 1
            return settleTicks >= 40 ? 1 : 0
        }
        if (phase === 0) {
            if (!ShellStore.ready || !locateSurface())
                return 0
            if (unavailableCheck) {
                check(!store.available && CoreClient.capabilities.indexOf("plugins.v1") < 0, "an older core leaves plugins unavailable")
                check(store.plugins.length === 0 && store.listRequestId === "", "no plugin request is sent without the capability")
                if (page !== null) {
                    check(find(page, "pluginsUnavailable").visible, "the desktop page explains plugins are unavailable")
                    check(!find(page, "pluginsInstalledPanel").visible, "the installed list is hidden")
                } else {
                    check(screen.rows.length === 1 && screen.rows[0].info === true, "Game Mode shows one unavailable row")
                }
                return 1
            }
            if (!store.loaded)
                return 0
            const signedInRun = Qt.application.arguments.indexOf("--plugins-signed-in") >= 0
            check(ShellStore.signedIn === signedInRun, "the sign-in state matches the run")
            if (page !== null && !signedInRun) {
                const host = find(root, "desktopSignedOutPlugins")
                check(host && host.visible, "signed-out desktop shows the standalone plugin manager")
                check(!find(root, "desktopSettingsScreen") || !find(root, "desktopSettingsScreen").visible,
                      "signed-out desktop doesn't expose the other settings pages")
            }
            check(CoreClient.capabilities.indexOf("plugins.v1") >= 0 && CoreClient.capabilities.indexOf("sources.catalog.v1") >= 0,
                  "CoreClient exposes negotiated plugin capabilities")
            check(store.plugins.length === 1, "only the built-in plugin is listed at first")
            const gfn = store.pluginById(gfnId)
            check(gfn && gfn.builtin === true && gfn.required === true && gfn.enabled === true && gfn.state === "ready",
                  "GeForce NOW is a required, running built-in plugin")
            check(!store.setEnabled(gfnId, false), "the required plugin can't be turned off from the shell")
            check(!store.uninstall(gfnId), "the built-in plugin can't be removed from the shell")
            if (screen !== null) {
                const sections = find(screen, "consoleSettingsSections")
                check(screen.sections.length === 9 && sections.count === 9, "Game Mode settings has nine sections")
                check(screen.selectedSection === 8, "the plugins route opens the Plugins section")
                check(find(screen, "consoleSettingsHeading").text === "Plugins", "the section heading reads Plugins")
                const pluginRow = screen.rows.find(row => row.pluginId === gfnId)
                check(pluginRow && pluginRow.plainText === true, "plugin rows render metadata as plain text")
                check(screen.rows[screen.rows.length - 1].info === true, "Game Mode points to Desktop mode for installs")
            } else {
                const row = find(page, "pluginRow-" + gfnId)
                check(row && row.textFormat === Text.PlainText, "desktop plugin rows render metadata as plain text")
                check(!find(page, "pluginToggle-" + gfnId).enabled, "the required plugin toggle is disabled")
            }
            startGeneration = store.generation
            check(store.inspectPackage("file:///tmp/opennow-fixture.opennow-plugin"), "inspection starts")
            phase = 1
            return 0
        }
        if (phase === 1) {
            if (store.inspection === null)
                return 0
            check(store.inspection.plugin.id === exampleId && store.inspection.token !== "", "inspection returns the candidate")
            check(store.plugins.length === 1, "inspection doesn't install anything")
            if (page !== null) {
                const consent = find(page, "pluginInstallConsent")
                check(consent && consent.opened, "the consent dialog opens after inspection")
                check(find(consent.contentItem, "pluginConsentDetails").textFormat === Text.PlainText, "candidate metadata is plain text")
                check(find(consent.contentItem, "pluginConsentWarning").text.indexOf("not sandboxed") >= 0, "the warning says the code is not sandboxed")
                if (showcaseStage === "consent")
                    return 1
                consent.accept()
            } else {
                check(store.commitInstall(), "commit starts")
            }
            phase = 2
            return 0
        }
        if (phase === 2) {
            if (!example())
                return 0
            if (showcaseStage === "installed")
                return 1
            check(store.inspection === null, "the inspection is consumed")
            check(store.generation > startGeneration, "the registry generation advances")
            check(example().enabled === false && example().state === "disabled" && example().trust === "unsigned-native",
                  "an installed plugin stays off until it is turned on")
            if (page !== null) {
                const toggle = find(page, "pluginToggle-" + exampleId)
                check(toggle && !toggle.checked && toggle.enabled, "the community plugin toggle is off and usable")
                toggle.valueChangedByUser(true)
            } else {
                screen.openPluginSheet(exampleId)
                const sheet = find(screen, "consolePluginSheet")
                check(screen.pluginSheetOpen && sheet.options.length === 3, "the plugin sheet offers turn on, browse and remove")
                check(sheet.options[1].disabled === true, "browsing waits until the plugin runs")
                sheet.chosen(0)
            }
            phase = 3
            return 0
        }
        if (phase === 3) {
            if (!example() || example().state !== "ready" || store.busyId !== "")
                return 0
            if (page !== null) {
                check(store.openPreview(exampleId), "the desktop preview opens")
            } else {
                screen.openPluginSheet(exampleId)
                const sheet = find(screen, "consolePluginSheet")
                check(sheet.options[1].disabled === false, "browsing is available for a running plugin")
                if (showcaseStage === "sheet")
                    return 1
                sheet.chosen(1)
            }
            phase = 4
            return 0
        }
        if (phase === 4) {
            if (store.previewLoading || store.previewItems.length === 0)
                return 0
            check(store.previewItems.length === 20 && store.previewNextCursor === "page-20", "the first page has 20 titles and a cursor")
            check(store.previewItems[0].title === "Example title 1", "titles come from the plugin")
            if (page !== null)
                check(find(page, "pluginCatalogPreview").visible, "the desktop preview panel is visible")
            else
                check(screen.pluginPreviewOpen && find(screen, "consolePluginPreviewList").count === 20, "the Game Mode preview lists titles")
            store.loadMorePreview()
            phase = 5
            return 0
        }
        if (phase === 5) {
            if (store.previewLoading || store.previewItems.length < 40)
                return 0
            check(store.previewItems.length === 40, "the next page is appended")
            store.searchPreview("title 4")
            phase = 6
            return 0
        }
        if (phase === 6) {
            if (store.previewLoading || store.previewQuery !== "title 4")
                return 0
            check(store.previewItems.length === 7 && store.previewNextCursor === null, "search restarts from the first page")
            if (showcase) {
                if (page !== null)
                    page.expandedId = exampleId
                return 1
            }
            store.searchPreview("special-ids")
            phase = 65
            return 0
        }
        if (phase === 65) {
            if (store.previewLoading || store.previewQuery !== "special-ids")
                return 0
            const ids = store.previewItems.map(item => item.localId).join(",")
            check(ids === "__proto__,constructor,toString,hasOwnProperty", "object-prototype names are ordinary item IDs: " + ids)
            store.searchPreview("")
            phase = 66
            return 0
        }
        if (phase === 66) {
            if (store.previewLoading || store.previewItems.length !== 20)
                return 0
            CoreClient.request("test.plugins.restart", {})
            phase = 67
            return 0
        }
        if (phase === 67) {
            if (!store.previewWaiting)
                return 0
            check(store.previewSourceId === exampleId && store.previewItems.length === 20 && !store.previewLoading,
                  "a restarting plugin keeps the preview and its titles without sending requests")
            CoreClient.request("test.plugins.restart", {})
            phase = 68
            return 0
        }
        if (phase === 68) {
            if (store.previewWaiting || store.previewLoading || store.previewItems.length !== 20)
                return 0
            check(store.previewSourceId === exampleId, "the preview reloads once the plugin is ready again")
            store.searchPreview("stale-source")
            phase = 69
            return 0
        }
        if (phase === 69) {
            if (store.previewLoading || store.previewWaiting || store.listRequestId !== "" || store.previewQuery !== "stale-source"
                    || store.previewItems.length === 0)
                return 0
            check(store.previewError === "" && store.previewSourceId === exampleId,
                  "a request that crosses a plugin restart refetches instead of failing")
            store.closePreview()
            check(store.previewSourceId === "" && store.previewItems.length === 0, "closing the preview clears it")
            if (page !== null) {
                page.removeId = exampleId
                const remove = find(page, "pluginRemoveConfirmation")
                remove.open()
                check(remove.opened, "removal asks for confirmation")
                remove.accept()
            } else {
                screen.pluginWarningId = exampleId
                screen.openWarning("plugin-remove")
                check(screen.warningOpen, "Game Mode asks before removing")
                screen.warningAction()
            }
            phase = 7
            return 0
        }
        if (phase === 7) {
            if (example() || store.busyId !== "")
                return 0
            check(store.plugins.length === 1 && store.error === "", "removal leaves only the built-in plugin")
            if (page === null || ShellStore.signedIn)
                return 1
            find(root, "signedOutPluginsBack").clicked()
            phase = 8
            return 0
        }
        if (phase === 8) {
            const signIn = find(root, "desktopSignInScreen")
            if (AppController.route === "settings-plugins")
                return 0
            check(!find(root, "desktopSignedOutPlugins").visible, "Back closes the plugin manager")
            check(signIn !== null && signIn.visible, "Back returns to sign-in")
            return 1
        }
        return 2
    }
}
