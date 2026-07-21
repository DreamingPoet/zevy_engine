# 阶段检查点：Rust/Android Clean-Machine 环境手册

## 元数据

- 完成时间：2026-07-21 17:40，Asia/Shanghai
- 分支 / HEAD：`main @ 7c5b9595bfdf0ca805f6231204360f4016fa9663`
- 阶段状态：文档与本机审计完成，未在第二台干净电脑端到端验证
- 提交状态：未提交

## 目标

为只参与 Rust/Bevy/OpenXR 开发的工程师提供一份从 clone 开始的 Windows 环境手册，使其能复现 PC 编译和 Android arm64 APK 构建、签名与验证流程，不依赖聊天历史或原开发机目录。

## 已完成

- 固化已验证版本：Rust/Cargo 1.95.0、cargo-apk 0.10.0、JDK 17、Platform 34、Build Tools 34.0.0、NDK 25.1.8937393。
- 记录 `aarch64-linux-android` target、MSVC host、Edition 2024 和 cargo-apk NativeActivity metadata。
- 记录直接 crates、Cargo.lock 和三个本地 patched crates 的职责。
- 提供 rustup、cargo-apk、sdkmanager、环境变量、keystore、cargo check/test、cargo apk build、zipalign、apksigner 和 adb 命令。
- 明确 cargo-apk deprecation、NDK 27 与 r25b 冲突、硬编码脚本路径、正式签名密钥等风险。
- 新增完成验收清单与常见故障章节。
- README 入口改为精确 Rust 1.95.0 并链接手册。

## 文件状态

本阶段修改：

- `Docs/Rust_Android_Environment_Setup.md`（新增）
- `zevy_engine/README.md`
- `Docs/Checkpoints/CURRENT.md`
- 本文件（新增）

任务开始时一度观察到用户的 `xr_render_scale: 0.8 -> 1.4` 工作区修改；随后它在本阶段未触碰 `config.rs` 的情况下由外部状态恢复。当前 diff 只包含上述文档相关文件，均未暂存、未提交。

## 验证

- 本机工具链版本和 rustup target/component 审计通过。
- cargo-apk 版本审计通过。
- Android 依赖文件路径及 r25b `source.properties` 检查通过。
- 文档环境检查 PowerShell 片段执行通过。
- `cargo metadata --locked --no-deps` 通过，并与文档版本/metadata 对齐。
- `git diff --check` 通过，仅有行尾转换提示。
- 未重装工具链、未清空 Cargo cache、未在第二台电脑完整构建，因此不能把 clean-machine 验收写成已通过。

## 关键决定

- 使用精确 `rustup override set 1.95.0-x86_64-pc-windows-msvc`，不用浮动 stable。
- 先复现 cargo-apk 0.10.0，再单独规划 xbuild 迁移。
- APK 首次复现固定 NDK r25b，不使用机器上更高版本代替。
- 手册命令不依赖原机 `F:\AndriodSDK\AndriodSDK`；已有脚本的硬编码路径作为风险明确说明。

## 下一步

在一台干净 Windows x64 电脑执行手册第 15 节全套验收，记录 Rust 安装、Cargo dependency fetch、Android linker、cargo-apk packaging 和签名阶段的实际结果。任何版本升级必须等基线复现成功后单变量进行。
