import QtQuick
import QtQuick.Controls
import OpenNOW

// Translation settings: screen OCR (EN source) -> Vietnamese overlay.
//
// Persistence: every persisted key lives in ShellStore.settings under
// "translation.<key>" and is written through settingsScreen.setSetting — the
// same path every other page uses. The C++ translationEngine mirrors those
// settings into its live properties. The Gemini API key is the exception: it
// is NEVER written to ShellStore.settings; it goes only to
// translationEngine.setGeminiApiKey() and this page only ever shows
// "Đã lưu key" / "Chưa có key", never the key value.
//
// Dropdowns use DesktopSettingsChoice (the repo's row-dropdown pattern: an
// expandable tile list with {label, value, detail} items). There is no
// row-level combo control in this codebase — DesktopSettingsDropdown is a
// global overlay menu, not a row control — so Choice is the faithful match.
Column {
    id: page
    objectName: "desktopTranslationSettings"
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; spacing: DesktopTokens.px(12)

    function t(key, fallback) { return page.settingsScreen.valueSetting("translation." + key, fallback) }
    function tb(key, fallback) { return page.settingsScreen.boolSetting("translation." + key, fallback) }
    // Push one saved value into the live engine (no settings write).
    function pushToEngine(key, value) {
        if (!page.hasEngine)
            return
        if (key === "enabled") translationEngine.enabled = value === true
        else if (key === "sourceLanguage") translationEngine.sourceLanguage = String(value)
        else if (key === "targetLanguage") translationEngine.targetLanguage = String(value)
        else if (key === "ocrEngine") translationEngine.ocrEngine = String(value)
        else if (key === "translationProvider") translationEngine.translationProvider = String(value)
        else if (key === "overlayMode") translationEngine.overlayMode = String(value)
        else if (key === "stabilityDelayMs") translationEngine.stabilityDelayMs = Number(value)
        else if (key === "fontSizePx") translationEngine.fontSizePx = Number(value)
        else if (key === "showOriginal") translationEngine.showOriginal = value === true
        else if (key === "cacheEnabled") translationEngine.cacheEnabled = value !== false
        else if (key === "dialoguePreset") translationEngine.setDialoguePreset(String(value))
    }
    function setT(key, value) {
        page.settingsScreen.setSetting("translation." + key, value)
        // Push the change to the live engine immediately (same reason as
        // TranslationOverlay.syncEngineFromSettings): the engine cannot
        // read ShellStore.settings itself, so a save alone would leave it
        // on stale values until the stream screen reloads.
        page.pushToEngine(key, value)
    }

    Component.onCompleted: {
        if (page.hasEngine) {
            page.keySaved = translationEngine.hasGeminiApiKey()
            // Sync saved settings into the engine now (push only — no
            // settings writes), so the Test section below uses the saved
            // engine/provider even before any stream.
            page.pushToEngine("enabled", page.tb("enabled", false))
            page.pushToEngine("sourceLanguage", String(page.t("sourceLanguage", "en")))
            page.pushToEngine("targetLanguage", String(page.t("targetLanguage", "vi")))
            page.pushToEngine("ocrEngine", String(page.t("ocrEngine", "gemini-vision")))
            page.pushToEngine("translationProvider", String(page.t("translationProvider", "google")))
            page.pushToEngine("overlayMode", String(page.t("overlayMode", "inplace")))
            page.pushToEngine("stabilityDelayMs", Number(page.t("stabilityDelayMs", 400)))
            page.pushToEngine("fontSizePx", Number(page.t("fontSizePx", 22)))
            page.pushToEngine("showOriginal", page.tb("showOriginal", false))
            page.pushToEngine("cacheEnabled", page.tb("cacheEnabled", true))
            page.pushToEngine("dialoguePreset", String(page.t("dialoguePreset", "wuthering-waves")))
        }
    }

    readonly property string ocrEngine: String(page.t("ocrEngine", "gemini-vision"))
    readonly property string provider: String(page.t("translationProvider", "google"))
    readonly property string overlayMode: String(page.t("overlayMode", "inplace"))

    // (a) Bật / Tắt
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("TRANSLATION") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "globe"; title: qsTr("Dịch màn hình")
            description: qsTr("Nhận diện chữ tiếng Anh trên màn hình stream rồi dịch sang tiếng Việt. Tắt = đúng bản gốc 100%.")
            showDivider: false
            DesktopSettingsToggle {
                objectName: "translationEnabledToggle"
                checked: page.tb("enabled", false)
                Accessible.name: qsTr("Dịch màn hình")
                onValueChangedByUser: value => page.setT("enabled", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Trạng thái")
            description: page.hasEngine
                ? String(translationEngine.statusText || qsTr("Sẵn sàng"))
                : qsTr("Translation engine chưa sẵn sàng (đang chờ phần C++). Cài đặt bên dưới vẫn lưu bình thường.")
            showDivider: false
        }
    }

    // (b) Ngôn ngữ
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("LANGUAGE") }
        DesktopSettingsChoice {
            objectName: "translationSourceLanguage"
            width: parent.width; glyph: "globe"; title: qsTr("Ngôn ngữ gốc")
            description: qsTr("Ngôn ngữ hiển thị trong game trên màn hình stream.")
            items: [
                {label: qsTr("English"), value: "en", detail: qsTr("Mặc định")},
                {label: qsTr("Korean"), value: "ko"},
                {label: qsTr("Japanese"), value: "ja"},
                {label: qsTr("Chinese"), value: "zh"}
            ]
            value: String(page.t("sourceLanguage", "en"))
            onSelected: value => page.setT("sourceLanguage", value)
        }
        DesktopSettingsChoice {
            objectName: "translationTargetLanguage"
            width: parent.width; glyph: "globe"; title: qsTr("Ngôn ngữ dịch")
            description: qsTr("Ngôn ngữ hiển thị của bản dịch.")
            showDivider: false
            items: [
                {label: qsTr("Tiếng Việt"), value: "vi", detail: qsTr("Mặc định")},
                {label: qsTr("English"), value: "en"},
                {label: qsTr("Français"), value: "fr"},
                {label: qsTr("Deutsch"), value: "de"}
            ]
            value: String(page.t("targetLanguage", "vi"))
            onSelected: value => page.setT("targetLanguage", value)
        }
    }

    // (c) OCR
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("OCR") }
        DesktopSettingsChoice {
            objectName: "translationOcrEngine"
            width: parent.width; glyph: "monitor"; title: qsTr("OCR engine")
            description: page.ocrEngine === "rapidocr" || page.ocrEngine === "windows-ocr"
                ? qsTr("Chưa khả dụng trong bản này — sẽ có ở bản sau.")
                : qsTr("Engine nhận diện chữ trên màn hình stream.")
            items: [
                {label: qsTr("Gemini Vision"), value: "gemini-vision", detail: qsTr("Mặc định · cần key Gemini")},
                {label: qsTr("Tesseract"), value: "tesseract", detail: qsTr("Offline · chạy trên máy")},
                {label: qsTr("RapidOCR"), value: "rapidocr", detail: qsTr("Chưa khả dụng trong bản này")},
                {label: qsTr("Windows OCR"), value: "windows-ocr", detail: qsTr("Chưa khả dụng trong bản này")}
            ]
            value: page.ocrEngine
            onSelected: value => page.setT("ocrEngine", value)
        }
        DesktopSettingsChoice {
            objectName: "translationDialoguePreset"
            width: parent.width; glyph: "image"; title: qsTr("Vùng thoại")
            description: qsTr("Vùng màn hình lấy chữ để dịch. Vùng thoại tuỳ chỉnh sẽ có ở bản sau; hiện chỉ chọn preset.")
            items: [
                {label: qsTr("Wuthering Waves"), value: "wuthering-waves", detail: qsTr("Vùng thoại dưới màn hình · mặc định")},
                {label: qsTr("Generic"), value: "generic", detail: qsTr("Vùng thoại chung")}
            ]
            value: String(page.t("dialoguePreset", "wuthering-waves"))
            onSelected: value => {
                page.setT("dialoguePreset", value)
                if (page.hasEngine)
                    translationEngine.setDialoguePreset(value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Độ ổn định")
            description: qsTr("Chờ chữ đứng yên bao lâu rồi mới dịch. Cao hơn = ít dịch nhầm khi chữ đang chạy, nhưng chậm hơn.")
            showDivider: false
            DesktopSettingsSlider {
                objectName: "translationStabilitySlider"
                accessibleName: qsTr("Độ ổn định")
                from: 200; to: 1000; stepSize: 10; suffix: " ms"
                value: Number(page.t("stabilityDelayMs", 400))
                onCommitted: value => page.setT("stabilityDelayMs", Math.round(value))
            }
        }
    }

    // (d) Dịch
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("TRANSLATE") }
        DesktopSettingsChoice {
            objectName: "translationProvider"
            width: parent.width; glyph: "globe"; title: qsTr("Dịch bằng")
            description: page.provider === "gemini" && !page.keySaved
                ? qsTr("Cần key Gemini — chưa có key. Dán key ở dưới.")
                : qsTr("Dịch vụ dịch bản OCR sang tiếng Việt.")
            items: [
                {label: qsTr("Google Translate"), value: "google", detail: qsTr("Miễn phí · mặc định")},
                {label: qsTr("Gemini"), value: "gemini", detail: qsTr("Cần key Gemini")}
            ]
            value: page.provider
            onSelected: value => page.setT("translationProvider", value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Gemini API key")
            description: (page.keySaved ? qsTr("Đã lưu key.") : qsTr("Chưa có key.")) + " "
                + qsTr("Key chỉ lưu trên máy này, không gửi đi đâu khác ngoài Google.")
            showDivider: false
            DesktopSettingsField {
                id: geminiKeyField
                objectName: "translationGeminiKeyField"
                echoMode: TextInput.Password
                placeholderText: qsTr("Dán key Gemini…")
                Accessible.name: qsTr("Gemini API key")
            }
            DesktopSettingsButton {
                objectName: "translationGeminiKeySaveButton"
                compact: true
                enabled: page.hasEngine && geminiKeyField.text !== ""
                text: qsTr("Lưu key")
                onClicked: {
                    if (!page.hasEngine)
                        return
                    translationEngine.setGeminiApiKey(geminiKeyField.text)
                    geminiKeyField.text = ""
                    page.keySaved = translationEngine.hasGeminiApiKey()
                }
            }
        }
    }

    // (e) Hiển thị
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("DISPLAY") }
        DesktopSettingsChoice {
            objectName: "translationOverlayMode"
            width: parent.width; glyph: "monitor"; title: qsTr("Cách hiển thị")
            description: page.overlayMode === "inplace"
                ? qsTr("Chèn bản dịch ngay tại vùng thoại, che chữ gốc.")
                : page.overlayMode === "subtitle"
                ? qsTr("Hiện bản dịch ở thanh phụ đề cuối màn hình, chữ gốc còn nguyên.")
                : qsTr("Không hiển thị bản dịch lên màn hình.")
            items: [
                {label: qsTr("Chèn tại chỗ"), value: "inplace", detail: qsTr("Mặc định")},
                {label: qsTr("Phụ đề"), value: "subtitle", detail: qsTr("Thanh cuối màn hình")},
                {label: qsTr("Tắt hiển thị"), value: "off"}
            ]
            value: page.overlayMode
            onSelected: value => page.setT("overlayMode", value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Cỡ chữ")
            description: qsTr("Cỡ chữ của bản dịch trên màn hình stream.")
            DesktopSettingsSlider {
                objectName: "translationFontSizeSlider"
                accessibleName: qsTr("Cỡ chữ")
                from: 16; to: 32; stepSize: 1; suffix: " px"
                value: Number(page.t("fontSizePx", 22))
                onCommitted: value => page.setT("fontSizePx", Math.round(value))
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Hiện cả chữ gốc")
            description: qsTr("Hiện chữ gốc nhỏ bên dưới bản dịch.")
            DesktopSettingsToggle {
                objectName: "translationShowOriginalToggle"
                checked: page.tb("showOriginal", false)
                Accessible.name: qsTr("Hiện cả chữ gốc")
                onValueChangedByUser: value => page.setT("showOriginal", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "check"; title: qsTr("Cache bản dịch")
            description: qsTr("Câu đã dịch rồi thì dùng lại ngay, không gọi API nữa.")
            DesktopSettingsToggle {
                objectName: "translationCacheToggle"
                checked: page.tb("cacheEnabled", true)
                Accessible.name: qsTr("Cache bản dịch")
                onValueChangedByUser: value => page.setT("cacheEnabled", value)
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "reset"; title: qsTr("Xoá cache")
            description: qsTr("Xoá toàn bộ câu đã dịch trong cache.")
            showDivider: false
            DesktopSettingsButton {
                objectName: "translationClearCacheButton"
                compact: true
                enabled: page.hasEngine
                text: qsTr("Xoá cache")
                onClicked: { if (page.hasEngine) translationEngine.clearCache() }
            }
        }
    }

    // (f) Kiểm tra
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("TEST") }
        DesktopSettingsField {
            id: testInputField
            objectName: "translationTestInput"
            width: parent.width
            text: "The Rover walks into the city, searching for answers."
            placeholderText: qsTr("Nhập câu tiếng Anh để dịch thử…")
            Accessible.name: qsTr("Câu dịch thử")
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "check"; title: qsTr("Dịch thử")
            description: qsTr("Dịch câu trên bằng đúng engine và provider đang chọn, không cần mở stream.")
            showDivider: false
            DesktopSettingsButton {
                objectName: "translationTestButton"
                compact: true
                enabled: page.hasEngine && testInputField.text !== ""
                text: qsTr("Dịch thử")
                onClicked: { if (page.hasEngine) translationEngine.requestTestTranslation(testInputField.text) }
            }
        }
        Text {
            visible: text !== ""
            width: parent.width
            text: page.hasEngine ? String(translationEngine.lastOriginalText || "") : ""
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
        Text {
            visible: text !== ""
            width: parent.width
            text: page.hasEngine ? String(translationEngine.lastTranslatedText || "") : ""
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            font.weight: Font.Bold
            wrapMode: Text.WordWrap
        }
        Text {
            visible: text !== ""
            width: parent.width
            text: {
                if (!page.hasEngine)
                    return qsTr("Translation engine chưa sẵn sàng.")
                const ocr = Number(translationEngine.lastOcrMs || 0)
                const tr = Number(translationEngine.lastTranslateMs || 0)
                let line = qsTr("OCR %1ms · Dịch %2ms · %3").arg(ocr).arg(tr)
                    .arg(translationEngine.lastCacheHit === true ? qsTr("Cache hit") : qsTr("Không dùng cache"))
                if (translationEngine.statusText)
                    line += "\n" + String(translationEngine.statusText)
                return line
            }
            color: Theme.textMuted
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
        Text {
            visible: text !== ""
            width: parent.width
            text: page.hasEngine ? String(translationEngine.lastError || "") : ""
            color: Theme.coral
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
    }
}
