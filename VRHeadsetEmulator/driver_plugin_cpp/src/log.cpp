#include "log.h"

#include <windows.h>

#include <chrono>
#include <fstream>
#include <mutex>
#include <sstream>

namespace virtual_hmd {
namespace {

std::mutex g_log_mutex;

std::string LogPath()
{
    char temp_path[MAX_PATH] = {};
    DWORD len = GetTempPathA(static_cast<DWORD>(sizeof(temp_path)), temp_path);
    if (len == 0 || len >= sizeof(temp_path)) {
        return "VRHeadsetEmulator_driver_cpp.log";
    }

    return std::string(temp_path) + "VRHeadsetEmulator_driver_cpp.log";
}

} // namespace

void Log(const char *message)
{
    Log(std::string(message));
}

void Log(const std::string &message)
{
    std::lock_guard<std::mutex> lock(g_log_mutex);

    const auto now = std::chrono::system_clock::now().time_since_epoch();
    const auto millis = std::chrono::duration_cast<std::chrono::milliseconds>(now).count();

    std::ofstream file(LogPath(), std::ios::app);
    if (!file) {
        return;
    }

    file << millis << " " << message << "\n";
}

} // namespace virtual_hmd

