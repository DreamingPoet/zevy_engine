#pragma once

#include "openvr_driver.h"
#include "pose_pipe.h"

#include <atomic>
#include <string>

namespace virtual_hmd {

class VirtualHmdDevice : public vr::ITrackedDeviceServerDriver, public vr::IVRDisplayComponent
{
public:
    explicit VirtualHmdDevice(PosePipe &pose_pipe);

    const std::string &SerialNumber() const;
    void PublishPose();

    vr::EVRInitError Activate(uint32_t object_id) override;
    void Deactivate() override;
    void EnterStandby() override;
    void *GetComponent(const char *component_name_and_version) override;
    void DebugRequest(const char *request, char *response_buffer, uint32_t response_buffer_size) override;
    vr::DriverPose_t GetPose() override;

    void GetWindowBounds(int32_t *x, int32_t *y, uint32_t *width, uint32_t *height) override;
    bool IsDisplayOnDesktop() override;
    bool IsDisplayRealDisplay() override;
    void GetRecommendedRenderTargetSize(uint32_t *width, uint32_t *height) override;
    void GetEyeOutputViewport(vr::EVREye eye, uint32_t *x, uint32_t *y, uint32_t *width, uint32_t *height) override;
    void GetProjectionRaw(vr::EVREye eye, float *left, float *right, float *top, float *bottom) override;
    vr::DistortionCoordinates_t ComputeDistortion(vr::EVREye eye, float u, float v) override;
    bool ComputeInverseDistortion(vr::HmdVector2_t *result, vr::EVREye eye, uint32_t channel, float u, float v) override;

private:
    void SetDeviceProperties();

    PosePipe &pose_pipe_;
    std::string serial_number_{"VRHeadsetEmulator_HMD_001"};
    std::atomic<uint32_t> object_id_{vr::k_unTrackedDeviceIndexInvalid};
    std::atomic<bool> active_{false};
};

} // namespace virtual_hmd

