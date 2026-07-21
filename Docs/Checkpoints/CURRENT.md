# 当前任务检查点：Rust 与 Android 构建环境手册

## 元数据

- 更新时间：2026-07-21 17:40，Asia/Shanghai
- 状态：文档编写和本机配置审计完成，等待用户审阅；未提交
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：`main @ 7c5b9595bfdf0ca805f6231204360f4016fa9663`，与 `origin/main` 一致
- 本阶段历史快照：`Docs/Checkpoints/2026-07-21-rust-android-environment-guide.md`
- 上一代码阶段：`Docs/Checkpoints/2026-07-21-static-light-mobility.md`

## 最终目标和完成标准

项目最终目标仍是建立面向 VR 一体机的高性能、现代、大量动态灯光/阴影渲染器。

当前文档任务要求另一台 Windows 电脑在 clone 后，只依赖仓库文档即可完成 Rust 桌面环境、Rust Android target、cargo-apk、SDK/NDK/JDK、签名和 APK 验证配置。完成标准：

1. 明确已验证的精确版本，不用会漂移的 `stable` 代替；
2. 覆盖 clean clone → Cargo 依赖 → Windows check/test → Android check → APK build/sign/verify 全流程；
3. 区分仓库 Rust 依赖、本地 patched crates、Android 外部工具和 PICO OpenXR loader；
4. 提供可复制 PowerShell 命令、验收清单和故障定位；
5. 只覆盖 Rust/Android 打包，不扩展到 UE 或内容生产。

## 已完成内容

### [本机实测审计]

- `rustc 1.95.0 (59807616e 2026-04-14)`，host `x86_64-pc-windows-msvc`。
- Cargo 1.95.0；rustup 1.29.0。
- 已安装 `aarch64-linux-android`、rustfmt、clippy、rust-src。
- cargo-apk 0.10.0；正确版本命令为 `cargo apk version`。
- Android 已验证基线：JDK 17.0.10、Platform 34、Build Tools 34.0.0、NDK 25.1.8937393/r25b。
- `Cargo.toml` metadata：arm64、min SDK 29、target SDK 34、assets、PICO OpenXR runtime lib。
- 仓库没有 `rust-toolchain.toml`；当前开发机依赖目录级 rustup stable override，文档改为要求精确 1.95.0 override。

### [文档实现]

- 新增 `Docs/Rust_Android_Environment_Setup.md`，包含：
  - 版本矩阵和支持边界；
  - Visual Studio Build Tools、rustup、Rust target/component 安装；
  - cargo-apk 0.10.0 精确安装和 deprecation 风险；
  - SDK/NDK/JDK 安装、环境变量和路径自检；
  - Cargo.lock、本地 Bevy/OpenXR fork 和 OpenXR loader 完整性检查；
  - Windows/Android check/test；
  - shipping、profiling、debug APK 构建；
  - debug/release keystore、zipalign、apksigner、ADB；
  - clean-machine 验收清单、故障排查和升级纪律。
- 更新 `zevy_engine/README.md`：把漂移的 `stable` 指令替换为 Rust 1.95.0，并链接完整手册。

### [官方资料核对]

- Rust 官方安装、rustup override 和 cross-compilation 文档。
- rust-mobile cargo-apk 上游及 0.10.0 发布 README。
- Android 官方 sdkmanager、特定 NDK、Build Tools 和 JDK 文档。

## 当前文件与 Git 状态

任务开始时 HEAD 已是用户提交 `7c5b959 添加静态灯光`。第一次审计曾看到用户把 `xr_render_scale` 从 `0.8` 改为 `1.4`；随后该 diff 在本阶段未编辑 `config.rs` 的情况下由外部状态恢复，当前实际工作树不再包含它。本阶段没有覆盖或提交该配置。

本阶段未暂存、未提交修改：

```text
 M Docs/Checkpoints/CURRENT.md
 M zevy_engine/README.md
?? Docs/Checkpoints/2026-07-21-rust-android-environment-guide.md
?? Docs/Rust_Android_Environment_Setup.md
```

## 关键决定与禁止事项

- 支持基线固定为 Rust/Cargo 1.95.0；不得把理论最低版本冒充为已验证版本。
- cargo-apk 0.10.0 虽被上游标记 deprecated，但仍是当前权威 APK 路径；禁止在新机初始化时静默换成 xbuild。
- APK 基线固定 NDK r25b。机器上存在 NDK 27 不代表 APK 应改用 27。
- `ANDROID_HOME` 与 `ANDROID_SDK_ROOT` 不得指向不同 SDK；当前推荐仅保留前者。
- 仓库脚本包含原开发机硬编码路径；手册优先提供不依赖该路径的手动命令，并明确机器路径改动不得提交。
- 不删除/替换 Zevy 的 Bevy/OpenXR local patches，不把 crates.io 同版本当成等价物。
- 开发 debug keystore 不能用于商店发布，密码和正式密钥不得进入仓库。

## 实际验证结果

### 已执行并通过

- `rustc -Vv`、`cargo -V`、`rustup show/target/component` 审计。
- `cargo apk version`：0.10.0。
- SDK/NDK/JDK/zipalign/apksigner/ADB 路径存在性检查。
- 文档中的 PowerShell Android 环境自检片段在本机通过，NDK 输出 25.1.8937393。
- `cargo metadata --locked --no-deps --format-version 1` 通过，版本、features、targets 和 Android metadata 与文档一致。
- `git diff --check` 通过，仅有 Windows LF→CRLF 提示。
- 本地 README 链接路径解析正确。

### 本任务未重新执行

- 未重装 Rust/cargo-apk/SDK/NDK/JDK，避免破坏已工作的机器环境。
- 未从空 Cargo cache 重新下载依赖。
- 未重新运行完整 PC test、APK release 构建或设备安装；文档中的这些命令来自当前仓库已验证流程，但新电脑仍必须按验收清单实际执行。

## 未完成步骤和唯一下一步

1. 用户审阅手册是否符合团队的新电脑目录约定。
2. 在真正的第二台/干净 Windows 电脑按第 15 节执行一次端到端验收。
3. 若通过，可后续把 `rust-toolchain.toml` 和 Android 脚本参数化作为独立工程改动提交。

唯一明确的下一步：**在第二台干净 Windows 电脑严格按 `Docs/Rust_Android_Environment_Setup.md` 执行，记录第一处不明确或失败的步骤，再据此修正文档；当前不要同时升级 cargo-apk 或 NDK。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Rust_Android_Environment_Setup.md`
4. `G:\zevy_engine\zevy_engine\Cargo.toml`
5. `G:\zevy_engine\zevy_engine\scripts\build_android_pico.ps1`
6. `G:\zevy_engine\zevy_engine\README.md`
7. 实际 `git status --short`、`git diff`、branch/HEAD
