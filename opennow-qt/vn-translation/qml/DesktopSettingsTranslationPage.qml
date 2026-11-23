import QtQuick
import QtQuick.Controls
import OpenNOW

// Translation settings: screen OCR (EN source) -> Vietnamese feathered dark-inpaint overlay.
Column {
    id: page
    objectName: "desktopTranslationSettings"
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; spacing: DesktopTokens.px(12)

    readonly property bool hasEngine: typeof translationEngine !== "undefined" && translationEngine !== null
    property bool keySaved: false
    property bool customKeySaved: false

    Component.onCompleted: {
        if (page.hasEngine) {
            page.keySaved = translationEngine.hasGeminiApiKey()
            page.customKeySaved = translationEngine.hasCustomApiKey()
        }
    }

    readonly property string ocrEngine: page.hasEngine ? String(translationEngine.ocrEngine || "windows-ocr") : "windows-ocr"
    readonly property string provider: page.hasEngine ? String(translationEngine.translationProvider || "google") : "google"
    readonly property string overlayMode: page.hasEngine ? String(translationEngine.overlayMode || "inplace") : "inplace"
    readonly property bool customConfigured: page.hasEngine
        && String(translationEngine.customApiBase || "") !== ""
        && page.customKeySaved
        && String(translationEngine.customApiModel || "") !== ""
    readonly property bool chunkDownloaded: page.hasEngine && translationEngine.chunkDownloaded === true
    readonly property bool canEnableOcr: page.hasEngine
        && (translationEngine.ocrReady === true
            || page.ocrEngine === "windows-ocr"
            || page.ocrEngine === "tesseract"
            || translationEngine.visionKeyConfigured === true)

    readonly property var customModelItems: {
        const models = page.hasEngine ? (translationEngine.customApiModels || []) : []
        if (models.length === 0) {
            const current = page.hasEngine ? String(translationEngine.customApiModel || "") : ""
            return current !== ""
                ? [{label: current, value: current}]
                : [{label: qsTr("Chưa lấy danh sách"), value: ""}]
        }
        return models.map(m => ({label: String(m), value: String(m)}))
    }

    // (a) Bật / Tắt & Trạng thái
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("TRANSLATION") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "globe"; title: qsTr("Dịch màn hình")
            description: qsTr("Nhận diện chữ tiếng Anh trên màn hình stream rồi khảm tối đè tiếng Việt tại chỗ. Tắt = đúng bản gốc 100%.")
            showDivider: false
            DesktopSettingsToggle {
                objectName: "translationEnabledToggle"
                checked: page.hasEngine && translationEngine.enabled
                enabled: page.canEnableOcr
                Accessible.name: qsTr("Dịch màn hình")
                onValueChangedByUser: value => { if (page.hasEngine) translationEngine.enabled = value }
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

    // (b) Dịch Chunk — Cơ sở dữ liệu 3,2 triệu từ Việt hóa sẵn
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("CHUNK") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "controller"; title: qsTr("Dịch Chunk")
            description: page.chunkDownloaded
                ? qsTr("Đã tải sẵn cơ sở dữ liệu — bật để dùng, không gọi AI, không tốn token.")
                : qsTr("Dịch bằng cơ sở dữ liệu Việt hoá sẵn — không gọi AI, không tốn token, hiện ngay. Tắt = dịch bằng provider đang chọn.")
            showDivider: false
            DesktopSettingsToggle {
                objectName: "translationChunkToggle"
                checked: page.hasEngine && translationEngine.chunkEnabled
                Accessible.name: qsTr("Dịch Chunk")
                onValueChangedByUser: value => { if (page.hasEngine) translationEngine.chunkEnabled = value }
            }
        }
        DesktopSettingsChoice {
            objectName: "translationChunkGame"
            width: parent.width; glyph: "controller"; title: qsTr("Game")
            description: qsTr("Game dùng chunk Việt hoá sẵn. Hiện chỉ có Wuthering Waves; game khác sẽ thêm sau.")
            items: [
                {label: qsTr("Wuthering Waves"), value: "wuthering-waves", detail: qsTr("3.8 · Lilies in a Dying Dream")}
            ]
            value: page.hasEngine ? String(translationEngine.chunkGame || "wuthering-waves") : "wuthering-waves"
            onSelected: value => { if (page.hasEngine) translationEngine.chunkGame = value }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Phiên bản chunk")
            description: page.hasEngine ? String(translationEngine.chunkGameVersion || "") : ""
            showDivider: false
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Đã dịch")
            description: page.hasEngine && String(translationEngine.chunkTranslatedText || "") !== ""
                ? String(translationEngine.chunkTranslatedText)
                : qsTr("Chưa tải — bấm Tải chunk để xem")
            showDivider: false
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Từ vựng")
            description: page.hasEngine ? String(translationEngine.chunkGameWordsText || "") : ""
            showDivider: false
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Trạng thái")
            description: page.hasEngine ? String(translationEngine.chunkStatus || qsTr("Chưa tải chunk")) : qsTr("Chưa tải chunk")
            showDivider: false
            DesktopSettingsButton {
                objectName: "translationChunkDownloadButton"
                compact: true
                enabled: page.hasEngine && !page.chunkDownloaded
                text: page.chunkDownloaded ? qsTr("Đã tải") : qsTr("Tải chunk")
                onClicked: { if (page.hasEngine) translationEngine.downloadChunks() }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "globe"; title: qsTr("Kho chunk")
            description: qsTr("Mã nguồn dữ liệu chunk trên GitHub.")
            showDivider: false
            Text {
                text: "<a href=\"" + (page.hasEngine ? String(translationEngine.chunkRepoUrl || "") : "") + "\">huyzoz/wuwa-vi-chunks</a>"
                color: Theme.label
                linkColor: Theme.focus
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.bodySize
                onLinkActivated: link => Qt.openUrlExternally(link)
            }
        }
    }

    // (c) OCR — Nhận diện chữ
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("OCR") }
        DesktopSettingsChoice {
            objectName: "translationOcrEngine"
            width: parent.width; glyph: "monitor"; title: qsTr("OCR engine")
            description: page.ocrEngine === "rapidocr"
                ? qsTr("Chưa khả dụng trong bản này — sẽ có ở bản sau.")
                : page.ocrEngine === "custom-vision" && !page.customConfigured
                ? qsTr("Cần base + key + model — điền ở mục API tùy chỉnh bên dưới.")
                : page.ocrEngine === "gemini-vision" && !page.keySaved
                ? qsTr("Cần key Gemini — dán key ở mục Dịch bên dưới hoặc chọn Windows OCR (Offline).")
                : qsTr("Engine nhận diện chữ trên màn hình stream.")
            items: [
                {label: qsTr("Windows OCR"), value: "windows-ocr", detail: qsTr("Mặc định · Offline siêu tốc ~15ms · Không cần key")},
                {label: qsTr("Tesseract"), value: "tesseract", detail: qsTr("Offline · chạy trên máy")},
                {label: qsTr("Gemini Vision"), value: "gemini-vision", detail: qsTr("Cloud · cần key Gemini")},
                {label: qsTr("API tùy chỉnh (Vision)"), value: "custom-vision", detail: qsTr("Cần model có vision")},
                {label: qsTr("RapidOCR"), value: "rapidocr", detail: qsTr("Chưa khả dụng trong bản này")}
            ]
            value: page.ocrEngine
            onSelected: value => { if (page.hasEngine) translationEngine.ocrEngine = value }
        }
        DesktopSettingsChoice {
            objectName: "translationDialoguePreset"
            width: parent.width; glyph: "image"; title: qsTr("Vùng thoại")
            description: qsTr("Vùng màn hình lấy chữ để dịch. Toàn màn hình dịch mọi chữ: thoại, nhiệm vụ, chat, menu.")
            items: [
                {label: qsTr("Toàn màn hình"), value: "full-screen", detail: qsTr("Mặc định · dịch mọi chữ trên màn hình")},
                {label: qsTr("Wuthering Waves"), value: "wuthering-waves", detail: qsTr("Chỉ vùng thoại dưới màn hình")},
                {label: qsTr("Generic"), value: "generic", detail: qsTr("Vùng thoại chung")}
            ]
            value: page.hasEngine ? String(translationEngine.dialoguePreset || "full-screen") : "full-screen"
            onSelected: value => { if (page.hasEngine) translationEngine.setDialoguePreset(value) }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Độ ổn định")
            description: qsTr("Chờ chữ đứng yên bao lâu rồi mới dịch. Cao hơn = ít dịch nhầm khi chữ đang chạy, nhưng chậm hơn.")
            showDivider: false
            DesktopSettingsSlider {
                objectName: "translationStabilitySlider"
                accessibleName: qsTr("Độ ổn định")
                from: 100; to: 1000; stepSize: 10; suffix: " ms"
                value: page.hasEngine ? Number(translationEngine.stabilityDelayMs || 220) : 220
                onCommitted: value => { if (page.hasEngine) translationEngine.stabilityDelayMs = Math.round(value) }
            }
        }
    }

    // (d) Hiển thị & Khảm tối tàng hình
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("DISPLAY") }
        DesktopSettingsChoice {
            objectName: "translationOverlayMode"
            width: parent.width; glyph: "monitor"; title: qsTr("Cách hiển thị")
            description: page.overlayMode === "inplace"
                ? qsTr("Khảm tối mờ biên tàng hình ngay tại vị trí chữ gốc, tự động khớp màu chữ game.")
                : page.overlayMode === "subtitle"
                ? qsTr("Hiện bản dịch ở thanh phụ đề cuối màn hình, chữ gốc còn nguyên.")
                : qsTr("Không hiển thị bản dịch lên màn hình.")
            items: [
                {label: qsTr("Chèn tại chỗ"), value: "inplace", detail: qsTr("Mặc định · Khảm tối tàng hình")},
                {label: qsTr("Phụ đề"), value: "subtitle", detail: qsTr("Thanh cuối màn hình")},
                {label: qsTr("Tắt hiển thị"), value: "off"}
            ]
            value: page.overlayMode
            onSelected: value => { if (page.hasEngine) translationEngine.overlayMode = value }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Cỡ chữ")
            description: qsTr("Cỡ chữ cơ sở của bản dịch trên màn hình stream.")
            DesktopSettingsSlider {
                objectName: "translationFontSizeSlider"
                accessibleName: qsTr("Cỡ chữ")
                from: 16; to: 32; stepSize: 1; suffix: " px"
                value: page.hasEngine ? Number(translationEngine.fontSizePx || 22) : 22
                onCommitted: value => { if (page.hasEngine) translationEngine.fontSizePx = Math.round(value) }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("Hiện cả chữ gốc")
            description: qsTr("Hiện chữ gốc nhỏ bên dưới bản dịch.")
            DesktopSettingsToggle {
                objectName: "translationShowOriginalToggle"
                checked: page.hasEngine && translationEngine.showOriginal
                Accessible.name: qsTr("Hiện cả chữ gốc")
                onValueChangedByUser: value => { if (page.hasEngine) translationEngine.showOriginal = value }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "speed"; title: qsTr("Hiện nhãn tốc độ OCR")
            description: qsTr("Mặc định tắt để màn hình sạch 100% như game Việt hoá native. Bật nếu muốn xem thông số ms.")
            DesktopSettingsToggle {
                objectName: "translationShowTimingPillToggle"
                checked: page.hasEngine && translationEngine.showTimingPill
                Accessible.name: qsTr("Hiện nhãn tốc độ OCR")
                onValueChangedByUser: value => { if (page.hasEngine) translationEngine.showTimingPill = value }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "check"; title: qsTr("Cache bản dịch")
            description: qsTr("Câu đã dịch rồi thì dùng lại ngay, không gọi API nữa.")
            DesktopSettingsToggle {
                objectName: "translationCacheToggle"
                checked: page.hasEngine && translationEngine.cacheEnabled
                Accessible.name: qsTr("Cache bản dịch")
                onValueChangedByUser: value => { if (page.hasEngine) translationEngine.cacheEnabled = value }
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

    // (e) Ngôn ngữ & Dịch dự phòng (khi câu nằm ngoài Chunk DB)
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
            value: page.hasEngine ? String(translationEngine.sourceLanguage || "en") : "en"
            onSelected: value => { if (page.hasEngine) translationEngine.sourceLanguage = value }
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
            value: page.hasEngine ? String(translationEngine.targetLanguage || "vi") : "vi"
            onSelected: value => { if (page.hasEngine) translationEngine.targetLanguage = value }
        }
    }

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("TRANSLATE") }
        DesktopSettingsChoice {
            objectName: "translationProvider"
            width: parent.width; glyph: "globe"; title: qsTr("Dịch bằng")
            description: page.provider === "gemini" && !page.keySaved
                ? qsTr("Cần key Gemini — chưa có key. Dán key ở dưới.")
                : page.provider === "custom" && !page.customConfigured
                ? qsTr("Cần base + key + model — điền ở mục API tùy chỉnh bên dưới.")
                : qsTr("Dịch vụ dự phòng cho các câu nằm ngoài Chunk DB.")
            items: [
                {label: qsTr("Google Translate"), value: "google", detail: qsTr("Miễn phí · mặc định")},
                {label: qsTr("Gemini"), value: "gemini", detail: qsTr("Cần key Gemini")},
                {label: qsTr("API tùy chỉnh"), value: "custom", detail: qsTr("Base + key riêng · stream")}
            ]
            value: page.provider
            onSelected: value => { if (page.hasEngine) translationEngine.translationProvider = value }
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

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("CUSTOM API") }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "globe"; title: qsTr("Base URL")
            description: qsTr("Base URL của API chuẩn OpenAI — vd https://api.openai.com/v1")
            showDivider: false
            DesktopSettingsField {
                id: customBaseField
                objectName: "translationCustomApiBaseField"
                text: page.hasEngine ? String(translationEngine.customApiBase || "") : ""
                placeholderText: qsTr("Dán base URL…")
                Accessible.name: qsTr("Base URL")
                onEditingFinished: {
                    if (page.hasEngine) translationEngine.customApiBase = text
                }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "info"; title: qsTr("API key")
            description: (page.customKeySaved ? qsTr("Đã lưu key.") : qsTr("Chưa có key.")) + " "
                + qsTr("Key chỉ lưu trên máy này, không gửi đi đâu khác ngoài base trên.")
            showDivider: false
            DesktopSettingsField {
                id: customKeyField
                objectName: "translationCustomApiKeyField"
                echoMode: TextInput.Password
                placeholderText: qsTr("Dán key API…")
                Accessible.name: qsTr("API key")
            }
            DesktopSettingsButton {
                objectName: "translationCustomApiKeySaveButton"
                compact: true
                enabled: page.hasEngine && customKeyField.text !== ""
                text: qsTr("Lưu key")
                onClicked: {
                    if (!page.hasEngine)
                        return
                    translationEngine.setCustomApiKey(customKeyField.text)
                    customKeyField.text = ""
                    page.customKeySaved = translationEngine.hasCustomApiKey()
                }
            }
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "check"; title: qsTr("Lấy danh sách model")
            description: qsTr("Danh sách model lấy từ /models của base trên, bằng key của bạn. Dùng được với mọi base chuẩn OpenAI (OpenAI, OpenRouter, Groq, DeepSeek…).")
            showDivider: false
            DesktopSettingsButton {
                objectName: "translationCustomApiFetchModelsButton"
                compact: true
                enabled: page.hasEngine && customBaseField.text !== "" && page.customKeySaved
                text: qsTr("Lấy danh sách model")
                onClicked: {
                    if (!page.hasEngine)
                        return
                    translationEngine.customApiBase = customBaseField.text
                    translationEngine.fetchCustomApiModels()
                }
            }
        }
        DesktopSettingsChoice {
            objectName: "translationCustomApiModel"
            width: parent.width; glyph: "monitor"; title: qsTr("Model")
            description: qsTr("Model đang dùng để dịch (và OCR nếu chọn Vision).")
            showDivider: false
            items: page.customModelItems
            value: page.hasEngine ? String(translationEngine.customApiModel || "") : ""
            onSelected: value => { if (page.hasEngine && value !== "") translationEngine.customApiModel = value }
        }
    }

    // (f) Kiểm tra nhanh
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
