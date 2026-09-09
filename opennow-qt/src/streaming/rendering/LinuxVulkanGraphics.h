#pragma once

#include <QByteArrayList>
#include <QtGlobal>
#include <QtGui/qtguiglobal.h>

#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
#include <QVulkanInstance>
#include <QVulkanFunctions>
#include "opennow_streamer_ffi.h"
#include <atomic>

class QQuickWindow;

namespace LinuxVulkanGraphics {
class Device final
{
public:
    struct Api {
        decltype(&opennow_streamer_vulkan_device_create) create = &opennow_streamer_vulkan_device_create;
        decltype(&opennow_streamer_vulkan_device_info) info = &opennow_streamer_vulkan_device_info;
        decltype(&opennow_streamer_vulkan_device_destroy) destroy = &opennow_streamer_vulkan_device_destroy;
    };

    Device();
    explicit Device(Api api);
    ~Device();
    Device(const Device &) = delete;
    Device &operator=(const Device &) = delete;

    bool initialize();
    bool adopt(QQuickWindow *window);
    bool adoptFallback(QQuickWindow *window);
    const OpenNowStreamerVulkanDevice *handle() const;
    QString lastError() const;
    static bool validInfo(const OpenNowStreamerVulkanDeviceInfo &info);
    static bool matchesContext(const OpenNowStreamerVulkanDeviceInfo &info,
                               const OpenNowStreamerGraphicsContext &context);

private:
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

inline bool hasDmabufImportContract(const QVersionNumber &instanceVersion,
                                   uint32_t physicalDeviceVersion,
                                   const QByteArrayList &requested,
                                   const QByteArrayList &available)
{
    if (instanceVersion < QVersionNumber(1, 1) || physicalDeviceVersion < VK_API_VERSION_1_1)
        return false;
    auto required = QByteArrayList{"VK_KHR_external_memory_fd", "VK_EXT_external_memory_dma_buf",
                                  "VK_EXT_image_drm_format_modifier"};
    if (instanceVersion < QVersionNumber(1, 2) || physicalDeviceVersion < VK_API_VERSION_1_2)
        required.append("VK_KHR_image_format_list");
    for (const auto &extension : required) {
        if (!requested.contains(extension) || !available.contains(extension)) return false;
    }
    return true;
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

inline uint32_t enabledImportCapabilities(QVulkanInstance *instance, VkPhysicalDevice physicalDevice)
{
    if (!extensionRequestInstalled.load(std::memory_order_acquire)
            || !instance || instance->apiVersion() < QVersionNumber(1, 1)) return 0;
    auto *functions = instance->functions();
    VkPhysicalDeviceProperties properties{};
    functions->vkGetPhysicalDeviceProperties(physicalDevice, &properties);
    if (properties.apiVersion < VK_API_VERSION_1_1) return 0;
    uint32_t count = 0;
    if (functions->vkEnumerateDeviceExtensionProperties(physicalDevice, nullptr, &count, nullptr)
            != VK_SUCCESS) return 0;
    QList<VkExtensionProperties> available(count);
    if (functions->vkEnumerateDeviceExtensionProperties(physicalDevice, nullptr, &count,
                                                       available.data()) != VK_SUCCESS) return 0;
    available.resize(count);
    QByteArrayList supported;
    for (const auto &property : available) supported.append(property.extensionName);
    const auto requested = qgetenv("QT_VULKAN_DEVICE_EXTENSIONS").split(';');
    uint32_t capabilities = 0;
    if (hasDmabufImportContract(instance->apiVersion(), properties.apiVersion, requested, supported))
        capabilities |= OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_IMPORT;
    if (hasDmabufBufferImportContract(instance->apiVersion(), properties.apiVersion, requested, supported))
        capabilities |= OPENNOW_STREAMER_GRAPHICS_CAP_VULKAN_DMABUF_BUFFER_IMPORT;
    return capabilities;
}
}
#endif
