# 当前任务检查点：多灯选灯连续性与随机阴影斑块修复

## 元数据

- 更新时间：2026-07-21 13:11，Asia/Shanghai
- 状态：阶段完成；`exact_lights=8` 已由用户在 Android/VR 中验证画面正常，本检查点与阶段代码同批提交
- 工作区：`G:\zevy_engine`
- 分支：`main`
- 阶段起始 HEAD：`08eced333d6698d84f74e2b47bf8d8ba6b4c93b9`；阶段提交以包含本文件的实际 `git rev-parse HEAD` 为准
- 当前设备：`PA9410MGJA190227G`（PICO A9210）
- 上一历史快照：`Docs/Checkpoints/2026-07-20-cyclopean-supercluster-preselection.md`
- 本阶段历史快照：`Docs/Checkpoints/2026-07-21-world-stable-lighting-exact8.md`

## 最终目标和完成标准

### 最终目标

建立面向 Android VR 一体机的高性能现代渲染引擎，支持大量动态 PointLight、动态阴影、稳定双眼输出和可编辑 UE Level 导入。允许 fork/修改 Bevy、wgpu、Naga、OpenXR、Vulkan backend 与 PBR/shadow pipeline。

### 产品完成标准

- Map_S03B 至少 16 盏 PointLight 的直接光和阴影同时存在。
- 灯光物理 `range` 与相机可见距离分离；禁止按相机距离突然开关灯或阴影。
- 转头、移动和远近观察时无屏幕 cluster 亮度块、世界空间随机阴影斑块或左右眼分裂。
- 低密度局部列表严格精确；高密度近似必须双眼一致、可重建、可回退，并有固定 A/B 证据。
- 20～30 分钟 thermal soak 后最终目标 P95 <= 13.89 ms（72 Hz），并向 11.11 ms（90 Hz）扩展。

### 当前阶段完成标准

- [用户已验证] 第一版 2x2 screen supercluster 的转头亮度块消失。
- [用户已证伪] 未重建的世界空间 shadow reservoir 会产生约 12.5 cm 明暗斑块，不能直接作为产品输出。
- [已实现/PC/Android] 真实 cluster `N <= exact_threshold` 时先严格求和，默认阈值 8。
- [用户已验证] `exact_lights=6` 仍有斑块；`exact_lights=8` 后斑块消失、画面正常，原屏幕块未回归。

## 已完成内容

### [上一阶段已实现并保留] Zevy bevy_pbr fork 与激进参考路径

- 本地 fork：`zevy_engine/third_party/crates/bevy_pbr-0.16.1`，Cargo `[patch.crates-io]` 强制全依赖图使用。
- storage cluster header 扩展为四个 `vec4<u32>`：原 offset/count ABI + 四个 PointLight ID + 四个 estimator weight。
- 2x2 XY、双眼 union 的 CPU supercluster 预选，不增加 binding/render pass。
- PICO 固定 A/B 曾把 Full GPU 30.29 ms 降到 23.78 ms，但用户移动验证发现屏幕块，因此只保留为 `world_reservoir=0, cluster_preselection=1, exact_lights=4` 的激进 A/B，不再是默认产品路径。

### [当前阶段已实现] 正确的分支顺序

- `zevy_pbr_functions.wgsl` 先判断真实 cluster 的 PointLight 数，再进入任何近似路径。
- 小列表不会再消费相邻 supercluster 的 union ID/权重。
- 单元测试锁定顺序：exact -> world reservoir -> aggressive screen supercluster。

### [当前阶段已实现；用户已验证运动问题消失] 单遍世界空间 reservoir

- 复杂列表一次真实 cluster 遍历同时选 2 个确定性 Hero 和 2 路 weighted streaming reservoir，避免 scalar reference 的第二次 O(N) 遍历。
- 随机场只依赖量化世界位置、light ID、stream 和可选 epoch，不依赖屏幕坐标、cluster ID、相机姿态或眼睛 ID。
- 每片元只建立一次 world seed；每候选只做一次 32-bit hash，再拆成两路 16-bit 随机数。
- sampled importance 在循环后按最终 ID 重算，减少循环内常驻寄存器。
- 用户确认：原“灯光交界处、转头时出现块状亮度变化，远处更明显”已经消失。

### [失败实验，必须保留] 原始随机 shadow 输出

- 用户截图：`C:\Users\idesi\Desktop\Screenshot_com.zevy.engine_2026.07.21-10.35.20.983_685.jpeg`。
- 地面/墙面出现规则明暗斑块，尺寸与 `floor(world_position * 8)` 的 12.5 cm cell 一致。
- 根因：二值/软阴影可见性直接进入 `C_l/(K p_l)`，单 realization 的高方差被 estimator weight 放大。
- 结论：缩小 cell 只会变噪点，扩大 cell 只会变大斑块；没有空间/时间重建时，raw shadow reservoir 不能作为产品路径。

### [当前修复已实现、部署并经用户验证] 可配置 exact local-list threshold

- 新配置：`RenderQualityConfig.point_light_exact_threshold`，默认 8，解析值不会低于 `Hero + Tail` 预算且上限 64。
- 新 shader 常量：`ZEVY_POINT_LIGHT_EXACT_THRESHOLD`。
- 当 `N <= max(exact_threshold, H+K)` 时，每盏灯的 BRDF、static shadow 与 dynamic overlay 全部严格求和。
- Android profiling 属性：`debug.zevy.exact_lights`。
  - `4`：复现 raw reservoir 性能/失败视觉档。
  - `6`：仍会进入 7～8 灯 overflow，用户验证仍有阴影斑块。
  - `8`：Map_S03B 当前默认且已通过 VR 视觉验证的保护档。
  - `16`：当前 16 灯地图的全精确质量参考。
- HUD Overview 显示 `Exact local list <= N lights`；Materials 页区分 exact 与 overflow BRDF 上限。
- 不改变灯光数、物理 `range`、shadow residency 或相机可见策略。

### [文档已更新]

- `zevy_engine/docs/VR_Renderring.md` 20.4.2/20.4.3：屏幕块数学模型、世界 reservoir、第二次失败实验、exact threshold 与下一代重建要求。
- `Docs/render_debug.md`：三档 Android 选灯 A/B 和 `debug.zevy.exact_lights`。

## 当前文件与 Git 状态

本阶段从 `main @ 08eced3` 的干净工作区开始；下列全部是连续 Zevy 多灯优化阶段改动，没有发现额外无关用户代码改动。状态列表是提交前快照；本文件与代码同批提交，恢复时必须以实际 `git status`、`git diff` 和 `git rev-parse HEAD` 为准，禁止 reset/checkout/覆盖未知改动。

```text
 M AGENTS.md
 M Docs/Checkpoints/CURRENT.md
 M Docs/render_debug.md
 M zevy_engine/Cargo.lock
 M zevy_engine/Cargo.toml
 M zevy_engine/docs/VR_Renderring.md
 M zevy_engine/src/app.rs
 M zevy_engine/src/config.rs
 M zevy_engine/src/lib.rs
 M zevy_engine/src/render_debug.rs
 M zevy_engine/src/scalable_lighting.rs
 M zevy_engine/src/shaders/zevy_pbr_functions.wgsl
?? Docs/Checkpoints/2026-07-20-cyclopean-supercluster-preselection.md
?? Docs/Checkpoints/2026-07-21-world-stable-lighting-exact8.md
?? zevy_engine/src/clustered_light_preselection.rs
?? zevy_engine/third_party/crates/bevy_pbr-0.16.1/
```

当前阶段直接相关文件：

- `zevy_engine/src/shaders/zevy_pbr_functions.wgsl`
- `zevy_engine/src/config.rs`
- `zevy_engine/src/scalable_lighting.rs`
- `zevy_engine/src/clustered_light_preselection.rs`
- `zevy_engine/src/render_debug.rs`
- `Docs/render_debug.md`
- `zevy_engine/docs/VR_Renderring.md`
- `Docs/Checkpoints/CURRENT.md`

## 关键决定、产品不变量与禁止事项

- 第一版 O(1) 2x2 screen supercluster 有实测性能价值，但已被用户视觉证伪，不能恢复为默认。
- raw world-space shadow reservoir 同样已被用户证伪；无重建时只能作为 overflow 研究路径。
- 当前 Map 的低风险保护是 exact <= 8；这不是最终 32/64 灯架构，下一代必须做双眼共享低分辨率 reservoir + edge-aware 重建，或确定性 Top-K + Tail proxy。
- 禁止靠扩大 `light.range`、删灯、相机距离开关或关闭远处阴影掩盖问题。
- 禁止把 estimator “期望无偏”当成单帧 VR 视觉可接受；必须检查方差、双眼与转头稳定性。
- 禁止把静态截图当作运动验收。
- 设备可用性由用户负责；任何 APK 安装失败、超时或掉线后必须立即停止并通知用户，不得自行反复改传输方案。
- PICO 系统截图动作可能暂停 NativeActivity/显示 Home；不要用它替代佩戴运动测试。

## 实际测试结果

### 已执行并通过

- `cargo fmt --all`。
- `cargo test --all-targets`：42 passed，0 failed。
- `cargo check --target aarch64-linux-android --message-format=short`。
- `cargo check --no-default-features --all-targets --message-format=short`。
- `cargo check --no-default-features --target aarch64-linux-android --message-format=short`。
- PC Map_S03B 实际启动，Naga/WGSL 编译并运行：
  - `target/render_debug/Map_S03B_world_reservoir_pc.png`
  - `target/render_debug/Map_S03B_world_reservoir_hashpair_pc.png`
  - `target/render_debug/Map_S03B_exact6_pc.png`（threshold 机制 PC 验证；最终默认已固化为 8）
  - `target/render_debug/Map_S03B_exact8_pc.png`（最终默认 8，Naga/WGSL 实际运行）
- Android release + `render_debug` APK 构建、zipalign 校验和签名通过。
- 最新 profiling APK 安装到 `PA9410MGJA190227G`，ADB 返回 `Success`。
- 用户在同一 APK 中把 `debug.zevy.exact_lights` 从 6 改为 8 并重启验证；当前已验证档为 direct=1、shadows=1、world_reservoir=1、cluster_preselection=0、exact_lights=8、Hero=2、Tail=2、dynamic overlay=1。
- 最终源码默认 8 的 release + `render_debug` APK 已重新构建、zipalign 校验并签名；为避免打扰已验证设备，没有重复安装。
- 最新进程正常运行，LayerCnt=1；未见 panic/FATAL。
- 当前非固定路径烟测窗口：CPU 约 8.8～9.7 ms，GPU 约 25.5～27.5 ms，599 MHz、约 60～61 C。它不是固定姿态 A/B，不能写成产品 P95 结论。

### 用户视觉结论

- [通过] screen-supercluster 灯光交界/转头块消失。
- [失败] exact threshold 4 的 raw world reservoir 出现世界空间阴影斑块。
- [失败] exact threshold 6 仍有斑块，证明当前测试位置存在 7～8 灯局部 overlap。
- [通过] exact threshold 8 后阴影斑块消失，画面正常；原 screen-supercluster 转头亮度块未回归。

### 无效/失败测试记录

- 第二台设备 `PA9410MGJ9260457G` 的 564 MB APK 普通安装超时，增量安装报 Windows bad file descriptor，push 后 broken pipe；没有把旧包误记为新包。
- `PA9410MGJA190227G` 一次 scalar A/B 时设备进入 `com.pvr.seethrough.setting`，LayerCnt=4、Zevy FrmGpu 约 0.2～1 ms；该窗口无效，未用于比较。
- 不同头部姿态下可见 mesh 19 与 39，不允许跨截图直接比较 GPU ms。
- APK 仍同时包含 `Map_S03B11.zip` 和展开资产，约 578 MB；是部署工程债，本阶段未擅自删除用户资产。

### 尚未执行

- exact 4/6/8/16 同一固定相机路径的 GPU P50/P95/P99。
- 当前相机路径每 cluster 真实 PointLight 数量 telemetry；此前只有 supercluster max=6。
- 32/64 灯增长曲线、20～30 分钟 thermal soak、AGI capture。
- overflow shadow reservoir 的 spatial/temporal reconstruction。

## 未完成步骤、风险和唯一下一步

1. 补真实 max lights/cluster telemetry，并采集 exact 4/6/8/16 固定路径 A/B，量化精确 shadowed BRDF 的边际成本。
2. 为 8 灯以上 overflow 实现双眼共享的低分辨率 shadow/lighting reservoir、edge-aware reconstruction 和固定路径误差报告；raw stochastic shadow 不再直出眼睛。
3. 继续 16→32→64 灯增长曲线、AGI capture 与 20～30 分钟 thermal soak。

唯一明确的下一步：**补齐真实 lights-per-cluster telemetry 与 exact 4/6/8/16 固定路径 A/B，然后为 8 灯以上 overflow 建立双眼共享重建路径；禁止再次直接输出 raw stochastic shadow。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\zevy_engine\docs\VR_Renderring.md`，重点 20.4.1～20.4.3
4. `G:\zevy_engine\zevy_engine\src\shaders\zevy_pbr_functions.wgsl`
5. `G:\zevy_engine\zevy_engine\src\config.rs`
6. `G:\zevy_engine\zevy_engine\src\scalable_lighting.rs`
7. `G:\zevy_engine\zevy_engine\src\clustered_light_preselection.rs`
8. `G:\zevy_engine\zevy_engine\src\render_debug.rs`
9. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_pbr-0.16.1\ZEVY_FORK.md`
10. 实际 `git status --short`、`git diff`、branch/HEAD 和设备属性
