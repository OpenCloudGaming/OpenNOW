// Minimal shim for <vulkan/vulkan.h> during bindgen only.
//
// hwcontext_vulkan.h #includes <vulkan/vulkan.h> unconditionally, but we don't
// want the whole Vulkan API in the bindings, nor a hard Vulkan-SDK build
// dependency.
// This shim defines only the types that FFmpeg internal Vulkan code references,
// so bindgen emits those AV structs with their real, linked-FFmpeg layout.
//
// VkPhysicalDeviceFeatures2 is the one struct embedded BY VALUE, so its
// size/alignment must match.
#ifndef VULKAN_H_
#define VULKAN_H_

#include <stdint.h>
#include <stddef.h>

typedef uint32_t VkBool32;
typedef uint32_t VkFlags;
typedef uint64_t VkFlags64;

// Dispatchable handles - pointers.
typedef struct VkInstance_T       *VkInstance;
typedef struct VkPhysicalDevice_T *VkPhysicalDevice;
typedef struct VkDevice_T         *VkDevice;

// Non-dispatchable handles - 64-bit on all platforms.
typedef uint64_t VkImage;
typedef uint64_t VkDeviceMemory;
typedef uint64_t VkSemaphore;

// Enums are int-width.
typedef int VkStructureType;
typedef int VkFormat;
typedef int VkImageTiling;
typedef int VkImageLayout;
typedef unsigned int VkQueueFlagBits; // bitmask (u32)
typedef int VkVideoCodecOperationFlagBitsKHR;
typedef int VkImageUsageFlagBits;
typedef int VkMemoryPropertyFlagBits;
typedef int VkAccessFlagBits; // libavutil 60 (FFmpeg 8.1): AVVkFrame.access

// Flag bitmasks.
typedef VkFlags   VkImageCreateFlags;
typedef VkFlags   VkImageUsageFlags;
typedef VkFlags   VkDeviceQueueCreateFlags;
typedef VkFlags64 VkAccessFlags2;
typedef VkFlags64 VkAccessFlagBits2; // libavutil 61 (FFmpeg 9.0): .access

// Only ever referenced through a pointer.
typedef struct VkAllocationCallbacks VkAllocationCallbacks;

// vkGetInstanceProcAddr loader pointer - pointer-sized.
typedef void (*PFN_vkVoidFunction)(void);
typedef PFN_vkVoidFunction (*PFN_vkGetInstanceProcAddr)(VkInstance, const char *);

// Embedded BY VALUE, so its size must be exact. Defined structurally (the frozen
// Vulkan 1.1 layout) so clang derives size = 240; _Static_assert pins it. The
// "55" is VkPhysicalDeviceFeatures's member count (frozen; grows via pNext).
typedef struct VkPhysicalDeviceFeatures2 {
    VkStructureType sType;
    void           *pNext;
    VkBool32        features[55];
} VkPhysicalDeviceFeatures2;
_Static_assert(sizeof(VkPhysicalDeviceFeatures2) == 240,
               "VkPhysicalDeviceFeatures2 must be 240 bytes (Vulkan 1.1 frozen ABI)");

#endif
