#pragma once

#include "openvr_driver.h"
#include "pose_pipe.h"
#include "virtual_hmd_device.h"

#include <memory>

namespace virtual_hmd {

class DeviceProvider : public vr::IServerTrackedDeviceProvider
{
public:
    vr::EVRInitError Init(vr::IVRDriverContext *driver_context) override;
    void Cleanup() override;
    const char *const *GetInterfaceVersions() override;
    void RunFrame() override;
    bool ShouldBlockStandbyMode() override;
    void EnterStandby() override;
    void LeaveStandby() override;

private:
    PosePipe pose_pipe_;
    std::unique_ptr<VirtualHmdDevice> hmd_;
};

} // namespace virtual_hmd

