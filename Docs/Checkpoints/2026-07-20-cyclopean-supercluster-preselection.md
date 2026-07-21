# 任务检查点：Cyclopean Supercluster 共享选灯突破

## 元数据

- 更新时间：2026-07-20 18:38，Asia/Shanghai
- 状态：实现与固定起点 PICO A/B 已完成；运动视觉、32/64 灯和 thermal soak 待下一阶段
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ 08eced3`
- 当前恢复入口：`Docs/Checkpoints/CURRENT.md`
- 本阶段历史快照：`Docs/Checkpoints/2026-07-20-cyclopean-supercluster-preselection.md`
- 上一阶段历史快照：`Docs/Checkpoints/2026-07-20-pico-ab-scalar-light-selection.md`

## 最终目标和完成标准

### 最终目标

建立面向 Android VR 一体机的高性能现代渲染引擎，支持大量动态 PointLight、动态阴影、PBR、稳定双眼输出和可编辑 UE Level 导入。为了目标允许 fork/修改 Bevy、wgpu、Naga、OpenXR 和 Vulkan backend。

### 产品完成标准

- Map_S03B 至少 16 盏 PointLight 的直接光和阴影同时存在。
- 灯光照射范围与相机可见距离分离；禁止靠扩大 `light.range` 或距离开关产生突然显隐。
- 左右眼共享灯光、阴影、LOD、随机样本和历史状态。
- 20～30 分钟 thermal soak 后 P95 ≤ 13.89 ms（72 Hz），支持设备向 11.11 ms（90 Hz）扩展。
- 优化必须有固定路径 A/B、GPU/CPU/P50/P95/P99、画质误差和真机证据。

### 本阶段完成标准

- [完成] 保留上一阶段 scalar 2H+2T reference，可在同一 APK 固定 A/B。
- [完成] 将 Hero/Tail 选择从 fragment 的两次 O(N) 扫描移到双眼共享 supercluster。
- [完成] 不增加 bind group/render pass，让 shader O(1) 读取四个标量 ID/权重。
- [完成] PC 和 Android shader/ABI 运行通过。
- [完成] PICO Direct/Full 与反向开关复测均显著胜出。
- [待下一阶段] 佩戴移动视觉、cluster 边界、32/64 灯斜率和 thermal soak。

## 已完成内容

### [实现] Zevy 本地 bevy_pbr fork

- 上一阶段已提交为 `08eced3`；本阶段开始时工作区干净。
- vendor `bevy_pbr 0.16.1` 到 `zevy_engine/third_party/crates/bevy_pbr-0.16.1`。
- `Cargo.toml [patch.crates-io]` 强制整个 Bevy 依赖图使用该 fork。
- 上游 Cargo SHA：`383b3510455c431f34cf3f2c6e3c2d40eddce744`。
- MIT/Apache-2.0 原许可证保留；`ZEVY_FORK.md` 记录 ABI 和回移要求。
- storage-buffer cluster header 从 `[vec4<u32>;2]` 扩成 `[vec4<u32>;4]`：
  - entry 0/1 完全保留 Bevy offset/count ABI；
  - entry 2 保存四个全局 PointLight ID；
  - entry 3 保存四个 f32 estimator weight 的 bit pattern。
- 不新增 mesh-view binding、descriptor 或 render pass。
- 额外容量约 32 bytes/cluster，即 4096 clusters 时约 128 KiB/view、双眼约 256 KiB。
- Uniform/WebGL cluster ABI 不变，返回 invalid selection 并自动回退 scalar shader。

### [实现] Cyclopean 2×2 supercluster 预选

新文件：`zevy_engine/src/clustered_light_preselection.rs`。

- 系统在 Bevy `AssignLightsToClusters` 之后运行。
- 所有 `XrCamera` 按 view index 排序并作为同一 Cyclopean 组。
- 相同 cluster index 的左右眼列表先取 union，再把 2×2 XY clusters 合为一个 supercluster。
- 同一 supercluster 的两个眼睛写入完全相同的四个 ID/权重。
- 候选 importance 使用 block 内所有 cluster center 的最大值，避免只按平均中心漏掉边缘强灯。
- 2 个 Hero 确定性选择；2 个 Tail 使用世界空间稳定 hash、系统重要性采样和 `1/(K p_l)` 权重。
- candidate union 包含每只眼/每个子 cluster 的原始列表；额外候选在不影响该 fragment 时自然衰减为零。
- temporal sampling 或 Hero+Tail > 4 时明确回退 scalar reference。
- HUD 报告 active/wait、XR view 数、非空 supercluster 平均候选数和最大候选数。
- 当前 PICO 固定起点：`XR 2`，非空 supercluster 平均候选约 3.1，最大 6。

### [实现] O(1) fragment shader 路径

- `clustered_forward.wgsl` 提供 `unpack_preselected_point_lights`。
- Zevy PBR shader 从 cluster header 读取一个 `vec4<u32>` ID 和一个 `vec4<f32>` weight。
- 四个灯用四段显式标量调用计算 BRDF/shadow；无局部数组、动态索引或候选循环。
- 缺失预选数据、关闭开关或不支持 storage buffer 时仍执行上一阶段约 2N scalar reference。
- `RenderQualityConfig.clustered_light_preselection` 默认开启。
- Android profiling property：`debug.zevy.cluster_preselection=0/1`，只在 `render_debug` build 中生效。
- 默认成本模型由
  `P[2N*c_importance + 4*c_shade]`
  变为
  `S*N*c_CPUselect + P[4*c_shade + 4*c_id]`，其中 `S≈clusters/4 << fragments`。

### [PC] 正确性验证

- Map_S03B 实际启动，fork 后 WGSL ABI 和 Zevy PBR shader 均由 Naga 编译运行。
- 39 assets、16 PointLight、96 estimated shadow views 正常加载。
- cluster preselection 日志和 HUD 生效。
- 截图：`target/render_debug/Map_S03B_cluster_preselection_pc.png`。
- 与 scalar reference 静态截图的整体灯光/阴影结构一致。
- 静态截图不能证明运动边界、双眼舒适度或时间稳定性。

### [Android/VR] 固定 A/B，设备 PA9410MGJ9260457G

同一 release + `render_debug` APK、Map_S03B 默认起点、GPU 599 MHz、约 60°C；每组预热 30～45 秒后采集 11～12 个 PICO `PxrMetric` 样本。

| A/B 档 | CPU avg | GPU avg | GPU P95 | 结果 |
|---|---:|---:|---:|---|
| Direct only，scalar fork reference | 5.31 ms | 20.89 ms | 21.74 ms | reference |
| Direct only，Cyclopean preselection | 5.27 ms | 17.18 ms | 17.55 ms | GPU -3.71 ms（-17.8%） |
| Full，scalar fork reference | 8.81 ms | 30.29 ms | 31.42 ms | reference |
| Full，Cyclopean preselection | 8.05 ms | 23.78 ms | 24.30 ms | GPU -6.51 ms（-21.5%） |
| Full，反向关闭复测 | 8.91 ms | 30.98 ms | 32.28 ms | 回到 reference |
| Full，重新开启复测 | 8.12 ms | 24.20 ms | 25.78 ms | 恢复优化档 |

结论：

- 收益明显超过上一检查点定义的 2.29 ms kill criterion。
- 开→关→开可逆，不能用测试顺序或 DVFS 解释。
- Direct 和 Full 均胜出，Full 收益更大，推断还改善了 shader 控制流/occupancy 和 shadow lookup 周边耦合。
- CPU 未回退；16 灯/4096 cluster 下 CPU prototype 尚未成为瓶颈。
- PICO FPS 受 90/45/30 runtime pacing 档影响，裁决以 GPU ms/P95 为主。
- 静态双眼截图结构一致；用户佩戴移动测试仍是必须条件。

### [设备最终状态]

设备 `PA9410MGJ9260457G` 已安装最新 profiling APK、启动并保持：

- `point_direct=1`
- `point_shadows=1`
- `cluster_preselection=1`
- `hero_samples=2`
- `tail_samples=2`
- `dynamic_overlay=1`
- `shadow_updates=2`
- `shadow_hz=8`
- HUD overview

最终截图：`G:\zevy_engine\zevy_engine\target\render_debug\Pico3_Final_Cyclopean.png`。

## 当前文件和修改状态

当前 HEAD：`main @ 08eced3`。本阶段改动未暂存、未提交；开始时工作区干净，因此下列均属于本阶段，没有发现额外用户改动。不得 reset/checkout/覆盖。

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
?? zevy_engine/src/clustered_light_preselection.rs
?? zevy_engine/third_party/crates/bevy_pbr-0.16.1/
```

主要用途：

- `Cargo.toml/Cargo.lock`：本地 `bevy_pbr` patch。
- `third_party/crates/bevy_pbr-0.16.1`：cluster ABI、提取/上传和 WGSL helper。
- `clustered_light_preselection.rs`：双眼 union、2×2 分组、Hero/Tail selection 和统计。
- `config.rs/app.rs/lib.rs`：配置和插件注册。
- `scalable_lighting.rs/zevy_pbr_functions.wgsl`：编译期 A/B 与 O(1) 消费路径。
- `render_debug.rs/Docs/render_debug.md`：property 和 HUD 证据。
- `VR_Renderring.md/AGENTS.md`：架构结论和下一优先级。
- 未修改 Map_S03B Level JSON、导出资产或 UE 插件。

## 关键决定与禁止事项

### 已决定

- 本地 fork 已被真机收益证明有价值；“不修改 Bevy”不再是约束。
- 当前 CPU supercluster 是验证成本阶数的第一版，不是最终 GPU-driven 终点。
- scalar 2H+2T 必须长期保留为 reference/fallback。
- storage header 扩展优先于新增 binding/pass，因为它保持主 PBR pipeline 布局稳定。
- 默认开启 preselection，但用户运动测试若发现明显视觉问题，先用 property 关闭并保留证据，不删除实验路径。
- 下一步先解决运动/边界和 16→32→64 扩展性，再决定 CPU cache/hysteresis 或 compute 迁移。

### 产品不变量/禁止

- 禁止按相机距离开关已启用灯光或阴影。
- 禁止扩大 `light.range` 解决可见性。
- 禁止左右眼独立随机选灯；当前代码同 cluster/supercluster 写入相同 ID/权重。
- 禁止把同一表面可能落入相邻 cluster 的风险写成“已完全消除双眼不一致”；必须佩戴验证。
- 禁止以删灯、明显 popping、统一降分辨率冒充结构性收益。
- 禁止把不完整 Bevy GPU span 当整帧；Android 仍以 PICO runtime/AGI 为准。
- 禁止直接比较不同 GPU 频率样本。
- 禁止把静态截图写成运动稳定性、thermal soak 或 32/64 灯已通过。

## 测试结果

### 已执行并通过

- `cargo fmt --check`。
- `cargo test --all-targets`：39 passed，0 failed。
- `cargo check --message-format=short`。
- `cargo check --target aarch64-linux-android --message-format=short`。
- `cargo check --no-default-features --all-targets --message-format=short`。
- `cargo check --no-default-features --target aarch64-linux-android --message-format=short`。
- `git diff --check`：通过，仅 Windows LF→CRLF 提示。
- PC Map_S03B 实际运行和截图成功，Naga/WGSL 无错误。
- Android release + HUD APK 构建、zipalign、签名通过。
- 最新 APK 已增量部署并在 PICO 启动。
- Direct/Full scalar/preselection A/B 与反向复测完成。
- 最终 HUD 确认 `Cluster select ON / XR 2 / avgN 3.1 / max 6`。

### 非阻塞警告

- 第三方 `bevy_mod_openxr` mismatched lifetime syntax。
- Cargo 同名 lib/bin PDB filename collision。
- 部分 glTF `TEXCOORD_2/3` 未被 Bevy 消费。
- APK 仍同时含 `Map_S03B11.zip` 和展开资产，约 578 MB。

### 尚未完成

- 用户佩戴并移动：墙面/柱边/灯光交界/快速转头，检查 2×2 block 边界、左右眼相邻 cluster 和 Tail 切换。
- 固定相机运动路径、图像误差/差分和自动报告。
- 16→32→64 灯增长曲线；当前 Map 只有 16 灯。
- 20～30 分钟 thermal soak、P95/P99、reprojection/missed frame。
- AGI/厂商 capture 的 register/occupancy 和带宽证据。
- CPU selection 的独立 profile、dirty cache/hysteresis。
- Cyclopean compute/GPU scene 迁移。
- 16 shadow-enabled 对应当前约 36～42 resident faces 的 view 分配审计。
- Shipping APK 真机性能基线；Shipping 编译组合已通过。

## 未完成步骤和下一步

1. 用户先佩戴当前设备移动观察；若出现块边界/双眼差异，记录位置和动作。
2. 增加固定相机路径和 preselection debug visualization/ID heatmap，量化边界变化。
3. 加入测试用 32/64 灯扩展配置，测 GPU/CPU 增长斜率和 max candidates。
4. 对运动问题依次 A/B：world-space hysteresis、邻 block halo、persistent reservoir；禁止直接退回逐片元扫描。
5. 若 CPU 在 32/64 灯成为瓶颈，把同一 selection ABI 迁入一次 Cyclopean compute。
6. 完成 20 分钟 thermal soak 后再把约 24 ms 视为稳定产品基线。

唯一明确的下一步：**佩戴验证运动边界，同时实现固定路径和 32/64 灯扩展 A/B，确认这次 GPU 阶数收益在空间、灯数和时间三个维度都成立。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Checkpoints\2026-07-20-cyclopean-supercluster-preselection.md`
4. `G:\zevy_engine\zevy_engine\docs\VR_Renderring.md`，重点 20.2.2、20.4、20.4.1
5. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_pbr-0.16.1\ZEVY_FORK.md`
6. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_pbr-0.16.1\src\cluster\mod.rs`
7. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_pbr-0.16.1\src\render\clustered_forward.wgsl`
8. `G:\zevy_engine\zevy_engine\src\clustered_light_preselection.rs`
9. `G:\zevy_engine\zevy_engine\src\shaders\zevy_pbr_functions.wgsl`
10. `G:\zevy_engine\zevy_engine\src\config.rs`
11. `G:\zevy_engine\zevy_engine\src\render_debug.rs`
12. 实际 `git status --short`、`git diff`、`git diff --cached` 与 branch/HEAD
