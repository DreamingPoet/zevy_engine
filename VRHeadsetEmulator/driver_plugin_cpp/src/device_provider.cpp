#include "device_provider.h"

#include "log.h"

namespace virtual_hmd {

vr::EVRInitError DeviceProvider::Init(vr::IVRDriverContext *driver_context)
{
    Log("DeviceProvider Init entered");
    VR_INIT_SERVER_DRIVER_CONTEXT(driver_context);

    pose_pipe_.Start();
    hmd_ = std::make_unique<VirtualHmdDevice>(pose_pipe_);

    if (!vr::VRServerDriverHost()->TrackedDeviceAdded(
            hmd_->SerialNumber().c_str(),
            vr::TrackedDeviceClass_HMD,
            hmd_.get())) {
        Log("TrackedDeviceAdded returned false");
        return vr::VRInitError_Driver_Unknown;
    }

    Log("DeviceProvider Init completed");
    return vr::VRInitError_None;
}

void DeviceProvider::Cleanup()
{
    Log("DeviceProvider Cleanup");
    hmd_.reset();
    pose_pipe_.Stop();
    VR_CLEANUP_SERVER_DRIVER_CONTEXT();
}

const char *const *DeviceProvider::GetInterfaceVersions()
{
    return vr::k_InterfaceVersions;
}

void DeviceProvider::RunFrame()
{
    if (hmd_) {
        hmd_->PublishPose();
    }
}

bool DeviceProvider::ShouldBlockStandbyMode()
{
    return false;
}

void DeviceProvider::EnterStandby() {}

void DeviceProvider::LeaveStandby() {}

} // namespace virtual_hmd

