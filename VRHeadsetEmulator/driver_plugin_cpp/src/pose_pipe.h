#pragma once

#include "pose_protocol.h"

#include <atomic>
#include <mutex>
#include <thread>

namespace virtual_hmd {

class PosePipe
{
public:
    PosePipe();
    ~PosePipe();

    void Start();
    void Stop();
    HmdPoseData LatestPose() const;

private:
    void ThreadMain();
    void RunOneSession();

    mutable std::mutex pose_mutex_;
    HmdPoseData latest_pose_{};
    std::atomic<bool> running_{false};
    std::atomic<bool> started_{false};
    std::thread thread_;
};

} // namespace virtual_hmd

