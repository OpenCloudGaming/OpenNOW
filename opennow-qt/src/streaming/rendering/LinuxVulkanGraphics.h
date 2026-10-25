#pragma once

#include <QByteArrayList>
#include <QtGlobal>
#include <QtGui/qtguiglobal.h>

#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
#include <QVulkanInstance>
#include <QVulkanFunctions>
#include <QSGRendererInterface>
#include "opennow_streamer_ffi.h"
#include <atomic>
#include <optional>

class QQuickWindow;

namespace LinuxVulkanGraphics {
class Device final
{
public:
    struct SceneGraphDevice {
        QSGRendererInterface::GraphicsApi api = QSGRendererInterface::Unknown;
        std::optional<VkPhysicalDeviceProperties> physicalDevice;
    };

    struct Api {
        decltype(&opennow_streamer_vulkan_device_create) create = &opennow_streamer_vulkan_device_create;
        decltype(&opennow_streamer_vulkan_device_info) info = &opennow_streamer_vulkan_device_info;
        decltype(&opennow_streamer_vulkan_device_destroy) destroy = &opennow_streamer_vulkan_device_destroy;
        SceneGraphDevice (*sceneGraphDevice)(QQuickWindow *) = &Device::querySceneGraphDevice;
    };

    Device();
    explicit Device(Api api);
    ~Device();
    Device(const Device &) = delete;
    Device &operator=(const Device &) = delete;

    bool initialize();
    bool adopt(QQuickWindow *window);
    bool adoptFallback(QQuickWindow *window);
    bool validateHardwareRendering(QQuickWindow *window);
    const OpenNowStreamerVulkanDevice *handle() const;
    QString lastError() const;
    static bool validInfo(const OpenNowStreamerVulkanDeviceInfo &info);
    static bool matchesContext(const OpenNowStreamerVulkanDeviceInfo &info,
                               const OpenNowStreamerGraphicsContext &context);

private:
    static SceneGraphDevice querySceneGraphDevice(QQuickWindow *window);
    void reset();

    Api m_api;
    OpenNowStreamerVulkanDevice *m_device = nullptr;
    OpenNowStreamerVulkanDeviceInfo m_info{};
    QVulkanInstance m_instance;
    QString m_lastError;
};

inline std::atomic<bool> extensionRequestInstalled{false};
inline QByteArrayList deviceExtensions()
{
    return {"VK_KHR_external_memory", "VK_KHR_external_memory_fd",
            "VK_EXT_external_memory_dma_buf", "VK_EXT_image_drm_format_modifier",
            "VK_KHR_image_format_list", "VK_KHR_bind_memory2",
            "VK_KHR_get_memory_requirements2", "VK_KHR_sampler_ycbcr_conversion",
            "VK_KHR_maintenance1", "VK_EXT_queue_family_foreign"};
}

inline void requestDeviceExtensions()
{
    QVulkanInstance probe;
    if (probe.supportedApiVersion() < QVersionNumber(1, 1)) return;
    auto requested = qgetenv("QT_VULKAN_DEVICE_EXTENSIONS").split(';');
    for (const auto &extension : deviceExtensions()) {
        if (!requested.contains(extension)) requested.append(extension);
    }
    qputenv("QT_VULKAN_DEVICE_EXTENSIONS", requested.join(';'));
    extensionRequestInstalled.store(true, std::memory_order_release);
}

inline QByteArrayList dmabufImportContractGaps(const QVersionNumber &instanceVersion,
                                              uint32_t physicalDeviceVersion,
                                              const QByteArrayList &requested,
                                              const QByteArrayList &available)
{
    QByteArrayList gaps;
    if (instanceVersion < QVersionNumber(1, 1)) gaps.append("instance-api<1.1");
    if (physicalDeviceVersion < VK_API_VERSION_1_1) gaps.append("device-api<1.1");
    // The image import acquires and releases its DMA-BUF from
    // VK_QUEUE_FAMILY_FOREIGN_EXT, because the producer is V4L2/VA-API rather
    // than another Vulkan device, so the foreign queue family extension is part
    // of the contract just as it is for the buffer import below.
    auto required = QByteArrayList{"VK_KHR_external_memory_fd", "VK_EXT_external_memory_dma_buf",
                                  "VK_EXT_image_drm_format_modifier",
                                  "VK_EXT_queue_family_foreign"};
    if (instanceVersion < QVersionNumber(1, 2) || physicalDeviceVersion < VK_API_VERSION_1_2)
        required.append("VK_KHR_image_format_list");
    for (const auto &extension : required) {
        if (!available.contains(extension)) gaps.append(extension + "(unsupported)");
        else if (!requested.contains(extension)) gaps.append(extension + "(not-requested)");
    }
    return gaps;
}

inline bool hasDmabufImportContract(const QVersionNumber &instanceVersion,
                                   uint32_t physicalDeviceVersion,
                                   const QByteArrayList &requested,
                                   const QByteArrayList &available)
{
    return dmabufImportContractGaps(instanceVersion, physicalDeviceVersion, requested, available)
        .isEmpty();
}

inline bool hasDmabufBufferImportContract(const QVersionNumber &instanceVersion,
                                         uint32_t physicalDeviceVersion,
                                         const QByteArrayList &requested,
                                         const QByteArrayList &available)
{
    if (instanceVersion < QVersionNumber(1, 1) || physicalDeviceVersion < VK_API_VERSION_1_1)
        return false;
    const QByteArrayList required = {"VK_KHR_external_memory_fd", "VK_EXT_external_memory_dma_buf",
                                    "VK_EXT_queue_family_foreign"};
    for (const auto &extension : required) {
        if (!requested.contains(extension) || !available.contains(extension)) return false;
    }
    return true;
}

struct ImportContract {
    uint32_t capabilities = 0;
    QString summary;
};

inline ImportContract importContract(QVulkanInstance *instance, VkPhysicalDevice physicalDevice)
{
    if (!instance || !physicalDevice)
        return {0, QStringLiteral("vulkan-import instance-or-device=missing")};
    if (!extensionRequestInstalled.load(std::memory_order_acquire))
        return {0, QStringLiteral("vulkan-import extension-request=not-installed")};
    if (instance->apiVersion() < QVersionNumber(1, 1))
        return {0, QStringLiteral("vulkan-import instance-api=%1").arg(instance->apiVersion().toString())};
    auto *functions = instance->functions();
    VkPhysicalDeviceProperties properties{};
    functions->vkGetPhysicalDeviceProperties(physicalDevice, &properties);
    QByteArrayList supported;
    uint32_t count = 0;
    if (functions->vkEnumerateDeviceExtensionProperties(physicalDevice, nullptr, &count, nullptr)
            == VK_SUCCESS) {
        QList<VkExtensionProperties> available(count);
        if (functions->vkEnumerateDeviceExtensionProperties(physicalDevice, nullptr, &count,
                                                           available.data()) == VK_SUCCESS) {
            available.resize(count);
            for (const auto &property : available) supported.append(property.extensionName);
        }
    }
    const auto requested = qgetenv("QT_VULKAN_DEVICE_EXTENSIONS").split(';');
    ImportContract contract;
    const auto imageGaps = dmabufImportContractGaps(instance->apiVersion(), properties.apiVersion,
                                                    requested, supported);
    if (imageGaps.isEmpty())
        contract.capabilities |= OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT;
    if (hasDmabufBufferImportContract(instance->apiVersion(), properties.apiVersion, requested, supported))
        contract.capabilities |= OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT;
    contract.summary = QStringLiteral(
        "vulkan-import device=\"%1\" vendor=0x%2 id=0x%3 type=%4 api=%5.%6.%7 driver=0x%8 caps=0x%9 image-gaps=[%10]")
        .arg(QString::fromUtf8(properties.deviceName).left(96))
        .arg(properties.vendorID, 0, 16).arg(properties.deviceID, 0, 16)
        .arg(int(properties.deviceType))
        .arg(VK_API_VERSION_MAJOR(properties.apiVersion))
        .arg(VK_API_VERSION_MINOR(properties.apiVersion))
        .arg(VK_API_VERSION_PATCH(properties.apiVersion))
        .arg(properties.driverVersion, 0, 16)
        .arg(contract.capabilities, 0, 16)
        .arg(QString::fromLatin1(imageGaps.join(',')));
    return contract;
}
}
#endif
