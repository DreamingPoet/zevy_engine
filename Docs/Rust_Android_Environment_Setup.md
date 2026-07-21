# Zevy Rust 与 Android APK 构建环境配置手册

本文用于在一台新的 Windows 电脑上，从克隆 Zevy 仓库开始，配置并验证 Rust 桌面编译环境和 Rust Android APK 打包环境。

本文只覆盖仓库中的 Rust 工程：`<repo>\zevy_engine`。不覆盖 Unreal Engine、UE 导出器编译、内容制作、PICO 系统配置或商店发布流程。

## 1. 可复现基线

以下版本来自 2026-07-21 可正常编译 Zevy 的开发机和仓库配置。新电脑第一次配置时应严格使用这一组版本；完成基线构建后，才能单变量升级。

| 项目 | 已验证版本/值 | 要求 |
| --- | --- | --- |
| 操作系统 | Windows x64 | Windows 10/11 x64 |
| Rust host | `x86_64-pc-windows-msvc` | 必须使用 MSVC host |
| `rustc` | `1.95.0 (59807616e 2026-04-14)` | 精确固定为 1.95.0 |
| Cargo | `1.95.0 (f2d3ce0bd 2026-03-21)` | 随 Rust 1.95.0 安装 |
| rustup | `1.29.0` | 这是已验证版本；可使用兼容的更新 rustup |
| Rust Android target | `aarch64-linux-android` | 必装，项目只打包 arm64 |
| Cargo edition | `2024` | 由 `Cargo.toml` 声明 |
| `cargo-apk` | `0.10.0` | 精确安装，不自动升级 |
| JDK | 17；已验证 `17.0.10` | Android 打包与签名需要 |
| Android SDK Platform | `android-34` | 与 target SDK 34 对齐 |
| Android Build Tools | `34.0.0` | 提供 `aapt`、`zipalign`、`apksigner` |
| Android NDK | `25.1.8937393`，即 r25b | APK 可复现基线 |
| Android min SDK | 29 | 由 `Cargo.toml` 声明 |
| Android target SDK | 34 | 由 `Cargo.toml` 声明 |
| APK ABI | `arm64-v8a` | 来自 `aarch64-linux-android` |

重要说明：

- 仓库当前没有 `rust-toolchain.toml`，因此克隆后不会自动选择 Rust 版本。必须执行本文的目录 override 命令。
- 不要只执行 `rustup override set stable`。未来的 stable 会变化，不能保证与当前基线一致。
- Edition 2024 的语言版本下限不等于 Zevy 的受支持版本。当前只把实际通过 PC、Android check 和 APK 构建的 Rust 1.95.0 作为可复现基线。
- `cargo-apk 0.10.0` 的发布文档已标记该工具 deprecated，并建议迁移到 xbuild；但 Zevy 当前的 `[package.metadata.android]`、NativeActivity 和脚本都建立在 cargo-apk 上。新电脑初始化时必须先复现 cargo-apk 路径，迁移只能作为独立开发阶段进行。

## 2. 仓库中的 Rust 工程

仓库根目录和 Rust crate 目录不要混淆：

```text
<repo>\                         # 仓库根目录
├─ Docs\
├─ ue_project\
└─ zevy_engine\                 # 本文要编译的 Rust crate
   ├─ Cargo.toml
   ├─ Cargo.lock
   ├─ assets\
   ├─ src\
   ├─ scripts\
   └─ third_party\
```

所有 Cargo 构建命令应在 `<repo>\zevy_engine` 中运行。`rustup override` 建议设置在仓库根目录，使仓库内其他 Rust 工具也继承同一版本。

## 3. Windows 主机前置依赖

### 3.1 Git

安装 Git for Windows。仓库包含较深的 vendored crate 路径，建议启用 long paths：

```powershell
git config --global core.longpaths true
```

如果仓库使用私有远程或 Git LFS，凭据和 LFS 登录由仓库管理员另行提供；它们不属于 Rust 工具链。

### 3.2 Visual Studio Build Tools

安装 Visual Studio 2022 Build Tools，并在安装器中选择：

- Desktop development with C++；
- MSVC v143 x64/x86 build tools；
- Windows 10 SDK 或 Windows 11 SDK。

Rust 官方说明 Windows MSVC toolchain 需要 Visual Studio C++ Build Tools。安装完成后重新打开 PowerShell。

### 3.3 rustup

从 Rust 官方页面下载并运行 x64 `rustup-init.exe`：

- [Rust 官方安装页](https://www.rust-lang.org/tools/install)
- [rustup Windows 安装说明](https://rust-lang.github.io/rustup/installation/windows.html)

确认 `%USERPROFILE%\.cargo\bin` 已在 `PATH`：

```powershell
where.exe rustup
where.exe rustc
where.exe cargo
```

## 4. 安装并固定 Rust 1.95.0

在普通 PowerShell 中执行：

```powershell
rustup toolchain install 1.95.0-x86_64-pc-windows-msvc --profile default
rustup component add rust-src --toolchain 1.95.0-x86_64-pc-windows-msvc
rustup target add aarch64-linux-android --toolchain 1.95.0-x86_64-pc-windows-msvc
```

`default` profile 已包含 `rustfmt` 和 `clippy`；`rust-src` 不是普通构建的硬性依赖，但用于 IDE、源码跳转和引擎级调试，Zevy 开发机应安装。

进入克隆后的仓库根目录并设置精确 override：

```powershell
Set-Location <repo>
rustup override set 1.95.0-x86_64-pc-windows-msvc
```

rustup 的目录 override 会作用于该目录及其子目录。官方说明见 [rustup Overrides](https://rust-lang.github.io/rustup/overrides.html)。

验证：

```powershell
rustup show
rustc -Vv
cargo -V
rustup target list --installed
rustup component list --installed
```

关键输出必须包含：

```text
release: 1.95.0
host: x86_64-pc-windows-msvc
aarch64-linux-android
rustfmt-x86_64-pc-windows-msvc
clippy-x86_64-pc-windows-msvc
```

如果 `rustc -Vv` 不是 1.95.0，先修复 override，不要继续编译。

## 5. 安装 cargo-apk 0.10.0

使用相同 Rust 工具链安装精确版本：

```powershell
cargo +1.95.0 install cargo-apk --version 0.10.0 --locked --force
```

验证命令是 `cargo apk version`，不是 `cargo apk --version`：

```powershell
cargo apk version
```

预期输出：

```text
cargo-apk 0.10.0
```

上游说明和 manifest 字段参考：[rust-mobile/cargo-apk](https://github.com/rust-mobile/cargo-apk)；0.10.0 发布文档的 deprecation 提示见 [cargo-apk 0.10.0 README](https://docs.rs/crate/cargo-apk/0.10.0/source/README.md)。

不要在初始配置阶段换成 xbuild、cargo-ndk 或自建 Gradle 工程，否则得到的结果不能与当前 Zevy APK 基线直接比较。

## 6. 安装 Android SDK、NDK 与 JDK

Android Studio 不是必需的；只安装 Android SDK Command-line Tools 也可以。Google 官方说明 `sdkmanager` 位于 Command-line Tools 包中：[sdkmanager 文档](https://developer.android.com/tools/sdkmanager)。

以下示例使用：

```text
C:\Android\Sdk
```

作为 SDK 根目录。可以改到其他磁盘，但后续所有变量必须指向同一个实际目录。

### 6.1 安装 JDK 17

安装 x64 JDK 17。已验证版本为 JDK 17.0.10。记下 JDK 根目录，例如：

```text
C:\Program Files\Java\jdk-17.0.10
```

不要把 `JAVA_HOME` 指向 `bin` 子目录。Android 官方的 JDK 说明见 [Java versions in Android builds](https://developer.android.com/build/jdks)。

### 6.2 安装 Android Command-line Tools

从 Android Developers 下载 Windows Command-line Tools，并整理为：

```text
C:\Android\Sdk\cmdline-tools\latest\bin\sdkmanager.bat
```

目录层级错误是常见问题；不要出现重复的 `cmdline-tools\cmdline-tools\bin`。

### 6.3 安装精确 SDK/NDK 包

```powershell
$AndroidHome = "C:\Android\Sdk"
$SdkManager = Join-Path $AndroidHome "cmdline-tools\latest\bin\sdkmanager.bat"

& $SdkManager --sdk_root=$AndroidHome `
    "platform-tools" `
    "platforms;android-34" `
    "build-tools;34.0.0" `
    "ndk;25.1.8937393"

& $SdkManager --sdk_root=$AndroidHome --licenses
```

Android 官方建议为需要可复现构建的项目安装并选择特定 NDK 版本；side-by-side NDK 会放在 `<sdk>\ndk\<version>`：[安装特定 NDK](https://developer.android.com/studio/projects/install-ndk)。

Zevy 的 cargo-apk 路径不需要额外安装 CMake、Ninja 或 Gradle。只有未来引入外部 C/C++ CMake 工程时才需要它们。

## 7. 配置当前 PowerShell 会话

每次打开新 PowerShell 后，在构建 APK 前设置：

```powershell
$AndroidHome = "C:\Android\Sdk"
$JavaHome = "C:\Program Files\Java\jdk-17.0.10"
$NdkHome = Join-Path $AndroidHome "ndk\25.1.8937393"

$env:ANDROID_HOME = $AndroidHome
$env:JAVA_HOME = $JavaHome
$env:NDK_HOME = $NdkHome
$env:NDKROOT = $NdkHome

# ANDROID_SDK_ROOT 若存在，必须与 ANDROID_HOME 完全相同；
# 为避免 cargo-apk 读取到冲突路径，Zevy 当前脚本选择删除它。
Remove-Item Env:ANDROID_SDK_ROOT -ErrorAction SilentlyContinue

$env:PATH = @(
    (Join-Path $JavaHome "bin")
    (Join-Path $AndroidHome "platform-tools")
    (Join-Path $AndroidHome "build-tools\34.0.0")
    (Join-Path $AndroidHome "cmdline-tools\latest\bin")
    $env:PATH
) -join ";"
```

验证实际路径，不要只看变量是否存在：

```powershell
$RequiredPaths = @(
    "$env:JAVA_HOME\bin\java.exe"
    "$env:JAVA_HOME\bin\keytool.exe"
    "$env:ANDROID_HOME\platforms\android-34\android.jar"
    "$env:ANDROID_HOME\build-tools\34.0.0\zipalign.exe"
    "$env:ANDROID_HOME\build-tools\34.0.0\apksigner.bat"
    "$env:ANDROID_HOME\platform-tools\adb.exe"
    "$env:NDK_HOME\source.properties"
)

$Missing = $RequiredPaths | Where-Object { !(Test-Path -LiteralPath $_) }
if ($Missing) {
    $Missing
    throw "Android build dependency is missing."
}

java -version
adb version
Get-Content "$env:NDK_HOME\source.properties"
```

NDK 输出必须包含：

```text
Pkg.Revision = 25.1.8937393
```

当前仓库的 `scripts\build_android_pico.ps1` 写死了原开发机的 `F:\AndriodSDK\AndriodSDK` 路径。新电脑可以：

1. 优先使用本文的手动 `cargo apk build` 命令；或
2. 在本地修改脚本顶部的 `$AndroidHome`、`$JavaHome`、`$NdkHome`。

机器路径修改不应提交到 Git；长期应把脚本改造成参数化/环境变量优先，但这不属于本手册任务。

## 8. 克隆后恢复 Rust 依赖

```powershell
git clone <repository-url> <repo>
Set-Location <repo>
rustup override set 1.95.0-x86_64-pc-windows-msvc
Set-Location .\zevy_engine
```

先确认关键文件存在：

```powershell
$RequiredRepoFiles = @(
    ".\Cargo.toml"
    ".\Cargo.lock"
    ".\third_party\crates\bevy_pbr-0.16.1\Cargo.toml"
    ".\third_party\crates\bevy_mod_openxr-0.3.0\Cargo.toml"
    ".\third_party\crates\bevy_mod_xr-0.3.0\Cargo.toml"
    ".\third_party\pico_openxr_loader\arm64-v8a\libopenxr_loader.so"
)

$Missing = $RequiredRepoFiles | Where-Object { !(Test-Path -LiteralPath $_) }
if ($Missing) {
    $Missing
    throw "Clone is incomplete or the working directory is wrong."
}
```

下载 `Cargo.lock` 固定的 crates：

```powershell
cargo fetch --locked
```

网络需要访问 crates.io 和 Rust 下载服务。依赖下载完成后，可用 `--offline` 检查离线可重复性：

```powershell
cargo check --locked --offline
```

不要删除或替换以下本地 patch：

- `third_party/crates/bevy_pbr-0.16.1`；
- `third_party/crates/bevy_mod_openxr-0.3.0`；
- `third_party/crates/bevy_mod_xr-0.3.0`。

它们由根 `Cargo.toml` 的 `[patch.crates-io]` 使用，包含 Zevy 渲染器和 OpenXR 修改。仅从 crates.io 重新下载同版本不能替代这些 fork。

## 9. Rust 主要依赖

直接依赖以 `Cargo.toml` 为准，完整传递依赖及精确解析版本以 `Cargo.lock` 为准。

| crate | 当前约束 | 用途 |
| --- | --- | --- |
| `bevy` | `0.16.1` | ECS、窗口、资产、PBR、渲染和 Android NativeActivity |
| `bevy_mod_openxr` | `0.3.0`，本地 patch | OpenXR session/render integration |
| `bevy_mod_xr` | `0.3.0`，本地 patch | XR 抽象层 |
| `bevy_xr_utils` | `0.3.0` | XR 辅助功能 |
| `openxr` | `0.19`，仅 Android | OpenXR Rust binding |
| `ndk-context` | `0.1`，仅 Android | Android NativeActivity/NDK context |
| `image` | `0.25.9`，PNG only | 纹理与 mip 处理 |
| `serde` / `serde_json` | `1` | Zevy Level JSON |
| `thiserror` | `2` | 错误类型 |

项目禁用了 Bevy 默认 features，并显式启用 Vulkan/窗口/资产/PBR/Android 所需功能。不要为了“修复缺 crate”随意打开 Bevy 全部默认 features；这会改变 APK 体积、编译时间和渲染依赖图。

## 10. Windows 编译验证

在 `<repo>\zevy_engine` 执行：

```powershell
cargo fmt --all --check
cargo check --locked
cargo test --locked --all-targets
cargo check --locked --no-default-features --all-targets
```

桌面运行：

```powershell
cargo run --locked -- --desktop `
    --level=levels/Map_S03B/Map_S03B.zevy-level.json
```

看到 vendored `bevy_mod_openxr` 的 `mismatched_lifetime_syntaxes` warning 是当前已知非阻塞警告；新的 error 或其他 warning 仍需调查。

## 11. Android 交叉编译验证

先确认本会话使用 NDK r25b，然后执行：

```powershell
cargo check --locked --target aarch64-linux-android
cargo check --locked --no-default-features --target aarch64-linux-android
```

Rust 官方说明 `rustup target add` 只安装目标标准库，Android linker 等工具仍由 NDK 提供：[rustup Cross-compilation](https://rust-lang.github.io/rustup/cross-compilation.html)。

如果这一步找不到 Android linker：

1. 再次检查 `NDK_HOME` / `NDKROOT`；
2. 检查 `rustup target list --installed` 是否包含 `aarch64-linux-android`；
3. 确认当前 PowerShell 没有指向另一个 NDK；
4. 确认 `source.properties` 是 `25.1.8937393`。

## 12. 开发签名密钥

debug profile 可以由 cargo-apk 自动创建 `%USERPROFILE%\.android\debug.keystore`，但 release profile不会自动生成。当前 Zevy 设备迭代使用 debug keystore 给 release APK 签名；这只适用于开发测试。

如果文件不存在：

```powershell
$AndroidConfig = Join-Path $env:USERPROFILE ".android"
New-Item -ItemType Directory -Force $AndroidConfig | Out-Null

& "$env:JAVA_HOME\bin\keytool.exe" -genkeypair -v `
    -keystore "$AndroidConfig\debug.keystore" `
    -storepass android `
    -alias androiddebugkey `
    -keypass android `
    -dname "CN=Android Debug,O=Android,C=US" `
    -keyalg RSA `
    -keysize 2048 `
    -validity 10000
```

为当前 release 设备包设置：

```powershell
$env:CARGO_APK_RELEASE_KEYSTORE = "$env:USERPROFILE\.android\debug.keystore"
$env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD = "android"
```

正式发布必须使用独立 release keystore，密钥和密码不得写进仓库、脚本或日志。

## 13. 构建 APK

### 13.1 Shipping 风格 release

默认 feature 是 `render_debug`。最终性能包应关闭默认 feature：

```powershell
cargo apk build --lib --release --no-default-features
```

输出：

```text
target\release\apk\zevy_engine.apk
```

### 13.2 带调试 HUD 的 release profiling 包

```powershell
cargo apk build --lib --release
```

### 13.3 Debug APK

```powershell
cargo apk build --lib --no-default-features
```

输出：

```text
target\debug\apk\zevy_engine.apk
```

`cargo-apk 0.10.0` 的 `build` 子命令没有 `--locked` 参数。打包前先运行 `cargo fetch --locked`，打包后确认 `Cargo.lock` 没有意外变化：

```powershell
git diff -- Cargo.lock
```

Cargo metadata 已声明：

- package：`com.zevy.engine`；
- crate type：`cdylib`；
- ABI：`aarch64-linux-android`；
- assets：`assets`；
- runtime libs：`third_party/pico_openxr_loader`；
- min SDK 29、target SDK 34；
- NativeActivity + OpenXR/PICO intent/category。

## 14. 验证 APK

```powershell
$Apk = Resolve-Path ".\target\release\apk\zevy_engine.apk"
$ZipAlign = "$env:ANDROID_HOME\build-tools\34.0.0\zipalign.exe"
$ApkSigner = "$env:ANDROID_HOME\build-tools\34.0.0\apksigner.bat"

& $ZipAlign -c -v 4 $Apk
if ($LASTEXITCODE -ne 0) { throw "zipalign verification failed" }

& $ApkSigner verify --verbose --print-certs $Apk
if ($LASTEXITCODE -ne 0) { throw "APK signature verification failed" }
```

连接设备后可安装：

```powershell
adb devices -l
adb install -r ".\target\release\apk\zevy_engine.apk"
```

本文的环境完成标准是 APK 构建、对齐和签名校验通过。设备运行画面、OpenXR runtime 和性能验收属于后续运行测试。

## 15. 一次性验收清单

新电脑只有同时满足以下项目，才视为 Rust 环境配置完成：

- [ ] `rustc -Vv` 显示 1.95.0 + `x86_64-pc-windows-msvc`；
- [ ] `cargo -V` 显示 1.95.0；
- [ ] `cargo apk version` 显示 0.10.0；
- [ ] Android Rust target 已安装；
- [ ] JDK 17 可运行；
- [ ] Platform 34、Build Tools 34.0.0、NDK 25.1.8937393 均存在；
- [ ] 三个本地 patched crate 存在；
- [ ] PICO OpenXR loader `.so` 存在；
- [ ] `cargo fetch --locked` 成功；
- [ ] Windows `cargo check/test` 成功；
- [ ] Android target `cargo check` 成功；
- [ ] shipping 和 profiling 至少各构建一次；
- [ ] `zipalign` 和 `apksigner verify` 成功；
- [ ] `Cargo.lock` 没有意外变化。

## 16. 常见故障

### `cargo apk --version` 报 unexpected argument

使用：

```powershell
cargo apk version
```

### `cargo apk` 不存在

检查 `%USERPROFILE%\.cargo\bin` 是否在 PATH，然后重新安装精确版本：

```powershell
cargo +1.95.0 install cargo-apk --version 0.10.0 --locked --force
```

### Rust 版本不是 1.95.0

在仓库根执行：

```powershell
rustup override set 1.95.0-x86_64-pc-windows-msvc
rustup show
```

### 找不到 NDK 或 Android linker

检查是否错误使用了机器上另一个 NDK。Zevy APK 基线是：

```text
<ANDROID_HOME>\ndk\25.1.8937393
```

同时设置 `NDK_HOME` 和 `NDKROOT` 指向该目录。

### `ANDROID_HOME` 和 `ANDROID_SDK_ROOT` 冲突

只保留 `ANDROID_HOME`，或者确保两者完全相同。当前 Zevy 构建脚本会删除 `ANDROID_SDK_ROOT`，防止 cargo-apk 选择错误 SDK。

### 找不到 `android-34`、`zipalign` 或 `apksigner`

重新安装：

```powershell
& "$env:ANDROID_HOME\cmdline-tools\latest\bin\sdkmanager.bat" `
    --sdk_root=$env:ANDROID_HOME `
    "platforms;android-34" `
    "build-tools;34.0.0"
```

### Release keystore 不存在

按第 12 节生成开发 keystore，或设置正式签名环境变量。不要把 keystore 加入 Git。

### APK 能生成但 OpenXR 启动失败

先确认文件存在：

```powershell
Test-Path ".\third_party\pico_openxr_loader\arm64-v8a\libopenxr_loader.so"
```

它由 `runtime_libs` 打进 APK。该文件缺失不是 Rust crate 下载问题，而是 clone/仓库内容不完整。

### 第一次构建非常慢或占用大量磁盘

Bevy、wgpu、OpenXR 和 release LTO 都会产生较大的 `target`。这是正常现象。不要把 `target` 提交到 Git；清理前确认没有需要保留的 APK、截图或 profiler 产物。

## 17. 升级规则

Rust、Cargo.lock、cargo-apk、NDK、SDK Platform 和 Build Tools 不得在同一次实验中一起升级。每次只升级一个变量，并至少完成：

1. Windows fmt/check/test；
2. Android 两种 feature 配置的 check；
3. release APK 构建、对齐、签名；
4. PICO 启动和 OpenXR session；
5. 固定 Map 的画面与 GPU/CPU 基线对比；
6. 更新本文的“可复现基线”表。

在 cargo-apk → xbuild 迁移完成并验证前，cargo-apk 0.10.0 仍是 Zevy 的权威 APK 打包路径。

## 18. 官方参考

- [Install Rust](https://www.rust-lang.org/tools/install)
- [rustup Overrides](https://rust-lang.github.io/rustup/overrides.html)
- [rustup Cross-compilation](https://rust-lang.github.io/rustup/cross-compilation.html)
- [cargo-apk upstream](https://github.com/rust-mobile/cargo-apk)
- [cargo-apk 0.10.0 source README](https://docs.rs/crate/cargo-apk/0.10.0/source/README.md)
- [Android sdkmanager](https://developer.android.com/tools/sdkmanager)
- [Install and configure the NDK](https://developer.android.com/studio/projects/install-ndk)
- [Android SDK Build Tools](https://developer.android.com/tools/releases/build-tools)
- [Java versions in Android builds](https://developer.android.com/build/jdks)
