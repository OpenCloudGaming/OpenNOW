import QtQuick
import OpenNOW

QtObject {
    property Component codecComponent: Component {
        DesktopSettingsSegmented {
            selectedIndex: 1
            options: [{label:"Auto",value:"auto"},
                {label:"AV1",value:"av1",enabled:ShellStore.codecAvailable("av1")},
                {label:"H.264",value:"h264",enabled:ShellStore.codecAvailable("h264")}]
        }
    }
    property Component backendComponent: Component {
        DesktopSettingsChoice { width: 900; expanded: true; value: "d3d11"; items: ShellStore.videoBackendItems() }
    }
    function check(ok, message) { if (!ok) throw new Error("Backend availability: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function run(parent) {
        ShellStore.settings = {codec:"av1",nativeVideoBackend:"auto"}
        ShellStore.nativeRuntimeReady = true
        ShellStore.acceptNativeCapabilities({protocolVersion:6,videoBackends:[{
            backend:"d3d11",available:true,codecs:[
                {codec:"h264",available:true},{codec:"h265",available:true},{codec:"av1",available:false}]
        }]})
        check(!ShellStore.codecAvailable("av1") && ShellStore.codecAvailable("h264"), "legacy GPU codec support")
        const codecs = codecComponent.createObject(parent)
        const av1 = find(codecs, "settingsOption-av1")
        check(av1 && !av1.enabled && av1.opacity < 0.5 && !av1.on, "persisted unsupported codec must be gray")
        check(find(codecs, "settingsOption-auto").enabled, "Auto must remain selectable")
        const backend = backendComponent.createObject(parent)
        if (Qt.platform.os === "windows") {
            check(find(backend, "settingsChoice-d3d11").enabled, "supported DX11 must be selectable")
            check(!find(backend, "settingsChoice-d3d12").enabled, "unimplemented DX12 must stay disabled")
            check(!find(backend, "settingsChoice-vulkan").enabled, "unimplemented Windows Vulkan must stay disabled")
        }
        ShellStore.nativeRuntimeReady = false
        check(!ShellStore.codecAvailable("h264"), "stale capabilities during probe")
        check(!find(codecs, "settingsOption-h264").enabled, "pending codecs must be disabled")
        if (Qt.platform.os === "windows") {
            const dx11 = find(backend, "settingsChoice-d3d11")
            check(!dx11.enabled && dx11.opacity < 0.5 && !dx11.chosen, "persisted backend must gray out during probe")
        }
        ShellStore.nativeRuntimeReady = true
        ShellStore.acceptNativeCapabilities({protocolVersion:6,videoBackends:[{
            backend:"d3d11",available:true,codecs:[{codec:"h264",available:true},{codec:"av1",available:true}]
        }]})
        check(find(codecs, "settingsOption-av1").enabled && find(codecs, "settingsOption-av1").opacity === 1,
            "supported codec must re-enable after detection")
        ShellStore.settings = {codec:"av1",nativeVideoBackend:"d3d12"}
        check(!ShellStore.codecAvailable("av1"), "backend changes must immediately update codec support")
        ShellStore.settings = {codec:"av1", nativeVideoBackend:"auto"}
        check(!ShellStore.hdrDecoderAvailable(), "missing ten-bit capabilities must not enable HDR")
        ShellStore.acceptNativeCapabilities({protocolVersion:6,videoBackends:[{
            backend:"vaapi",available:true,codecs:[{codec:"av1",available:true,colorQualities:["8bit_420","10bit_420"]}]
        }]})
        check(ShellStore.hdrDecoderAvailable(), "advertised Linux ten-bit decode must enable HDR")
        ShellStore.acceptNativeCapabilities({protocolVersion:6,videoBackends:[{
            backend:"d3d11",available:true,codecs:[{codec:"h265",available:true,hdrSupported:false,colorQualities:["10bit_420"]}]
        }]})
        check(!ShellStore.hdrDecoderAvailable(), "explicit HDR rejection must override ten-bit decode")
        ShellStore.acceptNativeCapabilities({protocolVersion:6,videoBackends:[{
            backend:"d3d11",available:true,codecs:[{codec:"h265",available:true,hdrSupported:true}]
        }]})
        check(ShellStore.hdrDecoderAvailable(), "advertised Windows HDR conversion must enable HDR")
        ShellStore.settings = {codec:"h265", nativeVideoBackend:"auto", decoderPreference:"software"}
        check(!ShellStore.hdrDecoderAvailable(), "software decode must not enable HDR")
        ShellStore.nativeRuntimeReady = false
        check(!ShellStore.hdrDecoderAvailable(), "stale HDR capabilities must fail closed")
        codecs.destroy(); backend.destroy()
        return true
    }
}
