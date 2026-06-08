#pragma once

#include <cstdint>

namespace virtual_hmd {

constexpr const char *kPipeName = R"(\\.\pipe\SteamVRVirtualHmdPipe)";

struct HmdPoseData
{
    float position[3];
    float orientation[4];
    uint32_t connected;
};

static_assert(sizeof(HmdPoseData) == 32, "HmdPoseData must match the Rust IPC frame.");

} // namespace virtual_hmd

