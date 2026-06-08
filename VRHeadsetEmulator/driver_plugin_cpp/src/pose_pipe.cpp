#include "pose_pipe.h"

#include "log.h"

#include <windows.h>

#include <chrono>
#include <cstring>
#include <sstream>

namespace virtual_hmd {

PosePipe::PosePipe()
{
    latest_pose_.position[0] = 0.0f;
    latest_pose_.position[1] = 1.75f;
    latest_pose_.position[2] = -0.5f;
    latest_pose_.orientation[0] = 0.0f;
    latest_pose_.orientation[1] = 0.0f;
    latest_pose_.orientation[2] = 0.0f;
    latest_pose_.orientation[3] = 1.0f;
    latest_pose_.connected = 1;
}

PosePipe::~PosePipe()
{
    Stop();
}

void PosePipe::Start()
{
    running_.store(true, std::memory_order_release);
    bool expected = false;
    if (!started_.compare_exchange_strong(expected, true, std::memory_order_acq_rel)) {
        return;
    }

    thread_ = std::thread(&PosePipe::ThreadMain, this);
    Log("pose pipe thread started");
}

void PosePipe::Stop()
{
    running_.store(false, std::memory_order_release);

    if (thread_.joinable()) {
        // Wake a blocking ConnectNamedPipe/ReadFile without requiring controller_app to run.
        HANDLE client = CreateFileA(kPipeName, GENERIC_WRITE, 0, nullptr, OPEN_EXISTING, 0, nullptr);
        if (client != INVALID_HANDLE_VALUE) {
            CloseHandle(client);
        }
        thread_.join();
    }

    started_.store(false, std::memory_order_release);
}

HmdPoseData PosePipe::LatestPose() const
{
    std::lock_guard<std::mutex> lock(pose_mutex_);
    return latest_pose_;
}

void PosePipe::ThreadMain()
{
    Log("pose pipe loop entered");
    while (running_.load(std::memory_order_acquire)) {
        RunOneSession();
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
    Log("pose pipe loop exited");
}

void PosePipe::RunOneSession()
{
    HANDLE pipe = CreateNamedPipeA(
        kPipeName,
        PIPE_ACCESS_INBOUND,
        PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
        PIPE_UNLIMITED_INSTANCES,
        sizeof(HmdPoseData),
        sizeof(HmdPoseData),
        0,
        nullptr);

    if (pipe == INVALID_HANDLE_VALUE) {
        std::ostringstream message;
        message << "CreateNamedPipeA failed error=" << GetLastError();
        Log(message.str());
        std::this_thread::sleep_for(std::chrono::milliseconds(250));
        return;
    }

    Log("pose pipe waiting for controller");
    BOOL connected = ConnectNamedPipe(pipe, nullptr) ? TRUE : (GetLastError() == ERROR_PIPE_CONNECTED);
    if (!connected) {
        std::ostringstream message;
        message << "ConnectNamedPipe failed error=" << GetLastError();
        Log(message.str());
        CloseHandle(pipe);
        return;
    }

    Log("pose pipe controller connected");
    while (running_.load(std::memory_order_acquire)) {
        HmdPoseData pose{};
        DWORD bytes_read = 0;
        BOOL ok = ReadFile(pipe, &pose, sizeof(pose), &bytes_read, nullptr);
        if (!ok || bytes_read != sizeof(pose)) {
            std::ostringstream message;
            message << "ReadFile ended ok=" << ok << " bytes_read=" << bytes_read << " error=" << GetLastError();
            Log(message.str());
            break;
        }

        std::lock_guard<std::mutex> lock(pose_mutex_);
        latest_pose_ = pose;
    }

    DisconnectNamedPipe(pipe);
    CloseHandle(pipe);
    Log("pose pipe session closed");
}

} // namespace virtual_hmd

