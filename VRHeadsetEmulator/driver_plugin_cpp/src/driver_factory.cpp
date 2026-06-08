#include "device_provider.h"
#include "log.h"
#include "openvr_driver.h"

#include <cstring>

#if defined(_WIN32)
#define HMD_DLL_EXPORT extern "C" __declspec(dllexport)
#else
#define HMD_DLL_EXPORT extern "C" __attribute__((visibility("default")))
#endif

namespace {

virtual_hmd::DeviceProvider g_device_provider;

} // namespace

HMD_DLL_EXPORT void *HmdDriverFactory(const char *interface_name, int *return_code)
{
    if (interface_name == nullptr) {
        if (return_code) {
            *return_code = vr::VRInitError_Init_InterfaceNotFound;
        }
        virtual_hmd::Log("HmdDriverFactory null interface");
        return nullptr;
    }

    virtual_hmd::Log(std::string("HmdDriverFactory requested ") + interface_name);

    if (std::strcmp(interface_name, vr::IServerTrackedDeviceProvider_Version) == 0) {
        if (return_code) {
            *return_code = vr::VRInitError_None;
        }
        virtual_hmd::Log("HmdDriverFactory returned provider");
        return &g_device_provider;
    }

    if (return_code) {
        *return_code = vr::VRInitError_Init_InterfaceNotFound;
    }

    virtual_hmd::Log(std::string("HmdDriverFactory rejected ") + interface_name);
    return nullptr;
}

