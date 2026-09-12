#include "streaming/rendering/LinuxVulkanGraphics.h"

#if defined(Q_OS_LINUX) && QT_CONFIG(vulkan) && __has_include(<vulkan/vulkan.h>)
#include <QQuickGraphicsDevice>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <rhi/qrhi_platform.h>
#include <limits>

namespace LinuxVulkanGraphics {
Device::Device() : Device(Api{}) {}

Device::Device(Api api) : m_api(api) {}

Device::~Device()
{
    reset();
}

void Device::reset()
{
    m_instance.destroy();
    if (m_device) m_api.destroy(m_device);
    m_device = nullptr;
    m_info = {};
}

bool Device::validInfo(const OpenNowStreamerVulkanDeviceInfo &info)
{
    return info.version == OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION
        && info.struct_size >= sizeof(info)
        && info.instance && info.physical_device && info.device && info.graphics_queue
        && info.graphics_queue_family_index <= uint32_t(std::numeric_limits<int>::max())
        && info.graphics_queue_index <= uint32_t(std::numeric_limits<int>::max())
        && info.api_version >= VK_API_VERSION_1_1;
}

bool Device::matchesContext(const OpenNowStreamerVulkanDeviceInfo &info,
                           const OpenNowStreamerGraphicsContext &context)
{
    return validInfo(info) && context.graphics_api == OPENNOW_STREAMER_GRAPHICS_API_VULKAN
        && context.instance == info.instance && context.physical_device == info.physical_device
        && context.device == info.device && context.queue == info.graphics_queue
        && context.queue_family_index == info.graphics_queue_family_index;
}

bool Device::initialize()
{
    if (m_device) return true;
    if (!m_api.create || !m_api.info || !m_api.destroy) {
        m_lastError = QStringLiteral("The native Vulkan device API is incomplete.");
        return false;
    }
    const auto status = m_api.create(&m_device);
    if (status != OPENNOW_STREAMER_OK || !m_device) {
        m_lastError = QStringLiteral("Native Vulkan Video device creation failed (status %1).")
                          .arg(int(status));
        reset();
        return false;
    }
    m_info.version = OPENNOW_STREAMER_VULKAN_DEVICE_INFO_VERSION;
    m_info.struct_size = sizeof(m_info);
    const auto infoStatus = m_api.info(m_device, &m_info);
    if (infoStatus != OPENNOW_STREAMER_OK || !validInfo(m_info)) {
        m_lastError = QStringLiteral("The native Vulkan device returned incompatible device information (status %1).")
                          .arg(int(infoStatus));
        reset();
        return false;
    }
    m_instance.setApiVersion(QVersionNumber(VK_API_VERSION_MAJOR(m_info.api_version),
                                           VK_API_VERSION_MINOR(m_info.api_version),
                                           VK_API_VERSION_PATCH(m_info.api_version)));
    m_instance.setVkInstance(reinterpret_cast<VkInstance>(m_info.instance));
    if (!m_instance.create()) {
        m_lastError = QStringLiteral("Qt could not adopt the native Vulkan instance (status %1).")
                          .arg(int(m_instance.errorCode()));
        reset();
        return false;
    }
    m_lastError.clear();
    return true;
}

bool Device::adopt(QQuickWindow *window)
{
    if (!m_device || !window || window->isVisible() || window->isSceneGraphInitialized()
            || window->rendererInterface()->graphicsApi() != QSGRendererInterface::Vulkan) {
        m_lastError = QStringLiteral("The native Vulkan device must be adopted before the Qt window is exposed.");
        return false;
    }
    window->setVulkanInstance(&m_instance);
    window->create();
    if (!m_instance.supportsPresent(reinterpret_cast<VkPhysicalDevice>(m_info.physical_device),
                                    m_info.graphics_queue_family_index, window)) {
        m_lastError = QStringLiteral("The native Vulkan graphics queue cannot present to this window system.");
        return false;
    }
    window->setGraphicsDevice(QQuickGraphicsDevice::fromDeviceObjects(
        reinterpret_cast<VkPhysicalDevice>(m_info.physical_device),
        reinterpret_cast<VkDevice>(m_info.device), int(m_info.graphics_queue_family_index),
        int(m_info.graphics_queue_index)));
    return true;
}

bool Device::adoptFallback(QQuickWindow *window)
{
    if (m_device || !window || window->isVisible() || window->isSceneGraphInitialized()
            || window->rendererInterface()->graphicsApi() != QSGRendererInterface::Vulkan) {
        m_lastError = QStringLiteral("The fallback Vulkan instance must be adopted before the Qt window is exposed.");
        return false;
    }
    if (!m_instance.isValid()) {
        auto extensions = QRhiVulkanInitParams::preferredInstanceExtensions();
        extensions.append("VK_EXT_swapchain_colorspace");
        const auto supported = m_instance.supportedExtensions();
        extensions.removeIf([&](const QByteArray &extension) { return !supported.contains(extension); });
        m_instance.setApiVersion(QVersionNumber(1, 1));
        m_instance.setExtensions(extensions);
        if (!m_instance.create()) {
            m_lastError = QStringLiteral("Qt could not create the fallback Vulkan instance (status %1).")
                .arg(int(m_instance.errorCode()));
            return false;
        }
    }
    window->setVulkanInstance(&m_instance);
    m_lastError.clear();
    return true;
}

const OpenNowStreamerVulkanDevice *Device::handle() const
{
    return m_device;
}

QString Device::lastError() const
{
    return m_lastError;
}
}
#endif
