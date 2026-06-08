#include "virtual_hmd_device.h"

#include "log.h"

#include <algorithm>
#include <cstring>

namespace virtual_hmd {
namespace {

constexpr uint32_t kDisplayWidth = 2160;
constexpr uint32_t kDisplayHeight = 1200;
constexpr uint32_t kEyeWidth = kDisplayWidth / 2;
constexpr uint32_t kEyeHeight = kDisplayHeight;
constexpr uint32_t kRenderWidth = 1512;
constexpr uint32_t kRenderHeight = 1680;

vr::HmdQuaternion_t QuaternionIdentity()
{
    return {1.0, 0.0, 0.0, 0.0};
}

void WriteStringResponse(char *buffer, uint32_t buffer_size, const char *response)
{
    if (buffer == nullptr || buffer_size == 0) {
        return;
    }

    std::strncpy(buffer, response, buffer_size - 1);
    buffer[buffer_size - 1] = '\0';
}

} // namespace

VirtualHmdDevice::VirtualHmdDevice(PosePipe &pose_pipe) : pose_pipe_(pose_pipe) {}

const std::string &VirtualHmdDevice::SerialNumber() const
{
    return serial_number_;
}

vr::EVRInitError VirtualHmdDevice::Activate(uint32_t object_id)
{
    object_id_.store(object_id, std::memory_order_release);
    active_.store(true, std::memory_order_release);
    SetDeviceProperties();
    Log("VirtualHmdDevice Activate");
    return vr::VRInitError_None;
}

void VirtualHmdDevice::Deactivate()
{
    active_.store(false, std::memory_order_release);
    object_id_.store(vr::k_unTrackedDeviceIndexInvalid, std::memory_order_release);
    Log("VirtualHmdDevice Deactivate");
}

void VirtualHmdDevice::EnterStandby() {}

void *VirtualHmdDevice::GetComponent(const char *component_name_and_version)
{
    if (component_name_and_version != nullptr &&
        std::strcmp(component_name_and_version, vr::IVRDisplayComponent_Version) == 0) {
        Log("VirtualHmdDevice GetComponent IVRDisplayComponent");
        return static_cast<vr::IVRDisplayComponent *>(this);
    }

    if (component_name_and_version != nullptr) {
        Log(std::string("VirtualHmdDevice GetComponent rejected ") + component_name_and_version);
    }

    return nullptr;
}

void VirtualHmdDevice::DebugRequest(const char *, char *response_buffer, uint32_t response_buffer_size)
{
    WriteStringResponse(response_buffer, response_buffer_size, "VRHeadsetEmulator OK");
}

vr::DriverPose_t VirtualHmdDevice::GetPose()
{
    const HmdPoseData frame = pose_pipe_.LatestPose();
    const bool connected = frame.connected != 0;

    vr::DriverPose_t pose{};
    pose.poseTimeOffset = 0.0;
    pose.qWorldFromDriverRotation = QuaternionIdentity();
    pose.qDriverFromHeadRotation = QuaternionIdentity();
    pose.vecPosition[0] = frame.position[0];
    pose.vecPosition[1] = frame.position[1];
    pose.vecPosition[2] = frame.position[2];
    pose.qRotation = {
        static_cast<double>(frame.orientation[3]),
        static_cast<double>(frame.orientation[0]),
        static_cast<double>(frame.orientation[1]),
        static_cast<double>(frame.orientation[2]),
    };
    pose.result = connected ? vr::TrackingResult_Running_OK : vr::TrackingResult_Uninitialized;
    pose.poseIsValid = connected;
    pose.willDriftInYaw = false;
    pose.shouldApplyHeadModel = true;
    pose.deviceIsConnected = connected;
    return pose;
}

void VirtualHmdDevice::PublishPose()
{
    const uint32_t object_id = object_id_.load(std::memory_order_acquire);
    if (!active_.load(std::memory_order_acquire) || object_id == vr::k_unTrackedDeviceIndexInvalid) {
        return;
    }

    vr::DriverPose_t pose = GetPose();
    vr::VRServerDriverHost()->TrackedDevicePoseUpdated(object_id, pose, sizeof(vr::DriverPose_t));
}

void VirtualHmdDevice::SetDeviceProperties()
{
    const uint32_t object_id = object_id_.load(std::memory_order_acquire);
    vr::PropertyContainerHandle_t container = vr::VRProperties()->TrackedDeviceToPropertyContainer(object_id);

    vr::VRProperties()->SetStringProperty(container, vr::Prop_TrackingSystemName_String, "virtual_hmd");
    vr::VRProperties()->SetStringProperty(container, vr::Prop_ModelNumber_String, "Virtual C++ HMD");
    vr::VRProperties()->SetStringProperty(container, vr::Prop_SerialNumber_String, serial_number_.c_str());
    vr::VRProperties()->SetStringProperty(container, vr::Prop_ManufacturerName_String, "Zevy Engine");
    vr::VRProperties()->SetStringProperty(container, vr::Prop_ResourceRoot_String, "virtual_hmd");
    vr::VRProperties()->SetInt32Property(container, vr::Prop_DeviceClass_Int32, vr::TrackedDeviceClass_HMD);

    vr::VRProperties()->SetBoolProperty(container, vr::Prop_ContainsProximitySensor_Bool, true);
    vr::VRProperties()->SetFloatProperty(container, vr::Prop_UserIpdMeters_Float, 0.063f);
    vr::VRProperties()->SetFloatProperty(container, vr::Prop_DisplayFrequency_Float, 90.0f);
    vr::VRProperties()->SetFloatProperty(container, vr::Prop_SecondsFromVsyncToPhotons_Float, 0.011f);
    vr::VRProperties()->SetBoolProperty(container, vr::Prop_IsOnDesktop_Bool, true);
    vr::VRProperties()->SetBoolProperty(container, vr::Prop_DisplayDebugMode_Bool, true);
    vr::VRProperties()->SetUint64Property(container, vr::Prop_CurrentUniverseId_Uint64, 1);
    vr::VRProperties()->SetInt32Property(container, vr::Prop_ExpectedTrackingReferenceCount_Int32, 0);
    vr::VRProperties()->SetInt32Property(container, vr::Prop_ExpectedControllerCount_Int32, 0);
    vr::VRProperties()->SetBoolProperty(container, vr::Prop_DoNotApplyPrediction_Bool, true);

    vr::VRProperties()->SetFloatProperty(container, vr::Prop_LensCenterLeftU_Float, 0.5f);
    vr::VRProperties()->SetFloatProperty(container, vr::Prop_LensCenterLeftV_Float, 0.5f);
    vr::VRProperties()->SetFloatProperty(container, vr::Prop_LensCenterRightU_Float, 0.5f);
    vr::VRProperties()->SetFloatProperty(container, vr::Prop_LensCenterRightV_Float, 0.5f);

    Log("VirtualHmdDevice properties set");
}

void VirtualHmdDevice::GetWindowBounds(int32_t *x, int32_t *y, uint32_t *width, uint32_t *height)
{
    if (x) *x = 0;
    if (y) *y = 0;
    if (width) *width = kDisplayWidth;
    if (height) *height = kDisplayHeight;
}

bool VirtualHmdDevice::IsDisplayOnDesktop()
{
    return true;
}

bool VirtualHmdDevice::IsDisplayRealDisplay()
{
    return false;
}

void VirtualHmdDevice::GetRecommendedRenderTargetSize(uint32_t *width, uint32_t *height)
{
    if (width) *width = kRenderWidth;
    if (height) *height = kRenderHeight;
}

void VirtualHmdDevice::GetEyeOutputViewport(vr::EVREye eye, uint32_t *x, uint32_t *y, uint32_t *width, uint32_t *height)
{
    if (x) *x = (eye == vr::Eye_Left) ? 0 : kEyeWidth;
    if (y) *y = 0;
    if (width) *width = kEyeWidth;
    if (height) *height = kEyeHeight;
}

void VirtualHmdDevice::GetProjectionRaw(vr::EVREye, float *left, float *right, float *top, float *bottom)
{
    if (left) *left = -1.0f;
    if (right) *right = 1.0f;
    if (top) *top = -1.0f;
    if (bottom) *bottom = 1.0f;
}

vr::DistortionCoordinates_t VirtualHmdDevice::ComputeDistortion(vr::EVREye, float u, float v)
{
    vr::DistortionCoordinates_t coordinates{};
    coordinates.rfRed[0] = u;
    coordinates.rfRed[1] = v;
    coordinates.rfGreen[0] = u;
    coordinates.rfGreen[1] = v;
    coordinates.rfBlue[0] = u;
    coordinates.rfBlue[1] = v;
    return coordinates;
}

bool VirtualHmdDevice::ComputeInverseDistortion(vr::HmdVector2_t *result, vr::EVREye, uint32_t, float u, float v)
{
    if (result == nullptr) {
        return false;
    }

    result->v[0] = u;
    result->v[1] = v;
    return true;
}

} // namespace virtual_hmd

