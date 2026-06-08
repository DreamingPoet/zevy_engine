# VRHeadsetEmulator

程序目标：

1. 在 Windows 上提供一个可由 SteamVR 加载的虚拟 HMD 驱动。
2. 允许通过键盘和鼠标模拟 Headset 的 6DOF 空间移动。
3. 允许通过控制器程序切换虚拟 HMD 的连接和断开状态。

## 当前实现

- `driver_plugin_cpp`：C++ SteamVR 驱动 DLL，导出 `HmdDriverFactory`。
- `controller_app`：键盘/鼠标控制器 EXE。
- `hmd_protocol`：驱动和控制器共享的 IPC 数据结构。
- IPC 使用 Windows Named Pipe：`\\.\pipe\SteamVRVirtualHmdPipe`。
- `driver_plugin`：旧 Rust ABI 实验版，目前不作为默认打包驱动。

## 构建

```powershell
cd G:\zevy_engine\VRHeadsetEmulator
cargo build
```

## 打包 SteamVR 驱动

```powershell
.\scripts\package-driver.ps1 -Profile release -Driver cpp
```

输出目录：

```text
G:\zevy_engine\VRHeadsetEmulator\dist\virtual_hmd
```

## 注册 SteamVR 驱动

```powershell
.\scripts\register-driver.ps1
```

注册后重启 SteamVR，再启动控制器：

```powershell
.\target\release\controller_app.exe
.\target\debug\controller_app.exe
```

控制器会打开一个固定窗口。只有这个窗口处于激活状态时，键盘和鼠标输入才会影响虚拟 HMD；窗口失焦后会暂停输入并清空移动按键状态。

## 控制方式

- `W` / `S`：前进 / 后退
- `A` / `D`：左移 / 右移
- `E` / `Q`：上 / 下
- 按住鼠标右键并移动鼠标：旋转视角
- `R`：重置姿态
- `C`：切换虚拟 HMD 连接 / 断开

详细实现说明见 `README_IMPLEMENTATION.md`。
