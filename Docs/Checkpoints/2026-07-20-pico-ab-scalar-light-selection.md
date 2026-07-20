# 任务检查点：PICO A/B 分解与标量选灯优化

## 元数据

- 更新时间：2026-07-20 17:45，Asia/Shanghai
- 状态：当前阶段已完成；下一阶段为 Cyclopean tile/froxel 共享选灯
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ 6a09a9f`
- 当前恢复入口：`Docs/Checkpoints/CURRENT.md`
- 本阶段历史快照：`Docs/Checkpoints/2026-07-20-pico-ab-scalar-light-selection.md`
- 上一阶段历史快照：`Docs/Checkpoints/2026-07-20-wave-a-telemetry-implementation.md`

## 最终目标和完成标准

### 最终目标

建立面向 Android VR 一体机的高性能现代渲染引擎，支持大量动态 PointLight、动态阴影、PBR、稳定双眼输出、可编辑 UE Level 导入，并允许 fork/修改 Bevy、wgpu、Naga、OpenXR 和 Vulkan backend。

### 产品完成标准

- Map_S03B 至少 16 盏 PointLight 的直接光和阴影可同时存在。
- 灯光照射范围与相机可见距离分离；禁止靠扩大 `light.range` 或距离开关产生突然显隐。
- 左右眼共享灯光、阴影、LOD、随机样本和历史状态。
- 20～30 分钟 thermal soak 后 P95 ≤ 13.89 ms（72 Hz），支持设备向 11.11 ms（90 Hz）扩展。
- 性能优化必须有固定路径 A/B、GPU/CPU/P50/P95/P99、画质误差和真机证据。

### 本阶段完成标准

- [完成] 用 Pico runtime 四档 A/B 分解 Map_S03B 的 geometry/direct/shadow/full 成本。
- [完成] 将默认 2H+2T 的候选 importance 计算由约 `4N` 降为约 `2N`，并在目标设备验证收益。
- [完成] 提供 Android debug-only Hero/Tail 运行时分档，测出 Tail scan 与 Tail shade 成本。
- [完成] 记录失败的局部数组/动态索引实验及停止条件。
- [完成] 修正 Android HUD 对不完整 GPU timestamp 的错误瓶颈判断。
- [完成] 代码、PC shader、Android profiling APK、Shipping feature 组合通过验证。

## 已完成内容

### [实现] 固定真机 A/B 与运行时分档

- `RenderQualityConfig` 保留固定 direct/shadow 四档，并允许 `point_light_tail_samples=0` 作为明确的 Hero-only A/B 档。
- Android + `render_debug` 在 Bevy/plugin/shader 安装前读取以下 Bionic system properties：
  - `debug.zevy.hud_page`
  - `debug.zevy.point_direct`
  - `debug.zevy.point_shadows`
  - `debug.zevy.dynamic_overlay`
  - `debug.zevy.shadow_updates`
  - `debug.zevy.shadow_hz`
  - `debug.zevy.hero_samples`
  - `debug.zevy.tail_samples`
- 这些覆盖只存在于 Android profiling build；无 `render_debug` 的 Shipping 不包含该入口。
- 用 property 切 HUD 页替代 Android/PICO 会被系统拦截的 F4 keyevent。

### [Android/VR] 四档基线，设备 PA9410MGJA190227G

release + `render_debug`，Map_S03B 默认起点；每组约预热 30 秒、采集 12～13 个 `PxrMetric` 样本：

| 档位 | FPS avg | CPU avg | GPU avg | GPU P95 |
|---|---:|---:|---:|---:|
| Geometry/post floor | 88.25 | 4.60 ms | 8.54 ms | 8.86 ms |
| Direct only，原始约 4N | 30.00 | 5.03 ms | 26.26 ms | 27.58 ms |
| Shadow submission only | 67.83 | 7.53 ms | 9.82 ms | 10.07 ms |
| Full，原始约 4N | 29.38 | 8.17 ms | 30.79 ms | 33.37 ms |

结论：

- Direct-only 相对 floor 增加 17.72 ms，是最大单项。
- Shadow-only GPU 增量约 1.28 ms，CPU 增量约 2.93 ms。
- Full 相对 Direct-only 增加约 4.53 ms，包含 shadow generation、sampling 和耦合。
- 当前相机位置首先应消除逐片元候选扫描；shadow geometry 很多，但不是最大 GPU 增量。

### [失败实验/Android-VR] 通用数组 2N 路径

- 尝试用局部数组、动态索引和循环在一次 Tail walk 中求解 K 个样本。
- Full GPU 从 30.79 ms 恶化到 40.04 ms，FPS 29.38→22.58。
- 该实现已删除。
- 推断：Adreno 寄存器压力/occupancy 和动态索引代价超过减少的 ALU。
- 禁止未来仅凭“循环次数更少”重新引入；必须同时检查寄存器、occupancy 和目标机 GPU ms。

### [实现 + Android/VR] 标量 K=1/K=2 快路

- Hero 扫描同时累计总 importance；Tail 总和由减去 Hero importance 得到。
- K=2 的两个有序系统采样阈值在一次 Tail walk 中用标量变量求解。
- K=1 使用独立无动态 sample loop 的标量特化。
- 无局部数组、无动态索引；Hero 集、PDF、系统采样阈值和 estimator 权重保持一致。
- 默认 K=2 importance 计算约由 `4N` 降为 `2N`。
- 设备 PA9410MGJA190227G：
  - Direct only：26.26→19.88 ms GPU，-24.3%；FPS 30.00→48.58。
  - Full：30.79→26.24 ms GPU，-14.8%；FPS 29.38→34.58。
- 静态双眼截图结构一致；运动中的随机方差、阴影稳定性和舒适度仍需要用户佩戴验证。

### [Android/VR] 同频 Hero/Tail 成本，设备 PA9410MGJ9260457G

只采用 GPU 稳定在 599 MHz 的数据；首次 2H+1T 的 26.12 ms @ 490 MHz 被排除，避免 DVFS 假结论。

| Direct-only 档位 | FPS avg | GPU avg | GPU P95 |
|---|---:|---:|---:|
| 2H+0T | 45.00 | 16.99 ms | 17.19 ms |
| 2H+1T scalar | 44.25 | 20.74 ms | 22.08 ms |
| 2H+2T scalar | 41.18 | 22.20 ms | 22.75 ms |
| Full 2H+2T | 30.00 | 28.10 ms | 28.89 ms |

近似分解：

- 第二个 Tail shade：`22.20 - 20.74 = 1.46 ms/sample`。
- 一次共享 Tail scan：`(20.74 - 16.99) - 1.46 = 2.29 ms`。
- 继续减少 Tail 数是画质 trade-off；结构性下一步是把 Hero/Tail 候选选择搬到双眼共享 tile/froxel。

### [实现 + Android/VR] HUD 真实性修复

- PICO runtime Full 同时报告约 28.10 ms GPU，而 Bevy instrumented pass 只显示约 0.55～0.57 ms 顶部 pass/约 6 ms 分类 span；内部 timestamp 不是移动 XR 整帧时间。
- Android HUD 现在显示 `GPU spans (partial)`；span 总和远低于 frame 时不再声称 CPU/streaming bottleneck，而提示使用 runtime/AGI/vendor profiler。
- `Shadow-enabled` 与 `Cache faces R/D/U` 分开显示，禁止把 16 个启用阴影的灯误写为 16 个实际 resident cache。
- 最终设备截图中：16 shadow-enabled、cache faces 42/0/42，HUD 正确显示 external/runtime GPU timing required。

### [实现/文档]

- `zevy_engine/docs/VR_Renderring.md` 新增 PICO 四档表、4N→2N 推导、失败实验、同频 Hero/Tail 成本和 timestamp 限制。
- `Docs/render_debug.md` 记录 Android properties、partial GPU span 和 shadow telemetry 语义。
- APK 构建日志暴露重复资产：`assets/levels/Map_S03B11.zip` 约 282 MB，同时又包含展开后的 Map_S03B 约 280 MB；当前 profiling APK 约 578 MB。未在本阶段清理，避免改变测试内容，但应进入 packaging/streaming 后续任务。

## 当前文件和修改状态

当前所有修改均未暂存、未提交；没有发现用户在本阶段新增的无关改动。不得 reset、checkout、覆盖或重排这些改动。

```text
 M Docs/Checkpoints/CURRENT.md
 M Docs/render_debug.md
 M zevy_engine/docs/VR_Renderring.md
 M zevy_engine/src/app.rs
 M zevy_engine/src/config.rs
 M zevy_engine/src/render_debug.rs
 M zevy_engine/src/scalable_lighting.rs
 M zevy_engine/src/scene.rs
 M zevy_engine/src/shaders/zevy_pbr_functions.wgsl
 M zevy_engine/src/shadow_cache.rs
?? Docs/Checkpoints/2026-07-20-wave-a-telemetry-implementation.md
?? Docs/Checkpoints/2026-07-20-pico-ab-scalar-light-selection.md
```

文件用途：

- `src/app.rs`：Android profiling properties 在插件/shader 安装前覆盖质量配置。
- `src/config.rs`：direct/shadow A/B、Hero/Tail 0～8 分档和测试。
- `src/render_debug.rs`：四页 HUD、properties、P50/P95/P99、workload/counter、partial timestamp 修复。
- `src/scalable_lighting.rs`：将质量参数替换进 WGSL。
- `src/shaders/zevy_pbr_functions.wgsl`：Hero/Tail 标量 2N 路径。
- `src/scene.rs`、`src/shadow_cache.rs`：Wave A shadow/caster telemetry 与固定 residency 支持。
- `target/render_debug`：忽略的 PICO/PC 截图与实验产物，不在 Git 状态中。
- 未修改 Map_S03B Level JSON、导出资产或 UE 插件。

## 关键决定与禁止事项

### 已决定

- PICO runtime/AGI/vendor profiler 是 Android 整帧 GPU 权威；Bevy span 只做同一 instrumented scope 的相对 A/B。
- 默认继续使用 2H+2T 标量参考档；0T/1T 只作为诊断和可选质量 trade-off。
- 下一结构性突破是 Cyclopean tile/froxel 共享选灯，不再继续把希望寄托在逐片元循环微优化。
- 保留当前 scalar reference path，任何 tile/GPU-driven 路径必须可固定 A/B 和回退。
- PC 只裁决 shader/资源正确性；真机和 thermal soak 裁决性能。

### 产品不变量/禁止

- 禁止按相机距离开关已启用灯光或阴影。
- 禁止扩大 `light.range` 解决相机可见性。
- 禁止左右眼独立随机选灯、LOD、shadow 或历史。
- 禁止以删灯、明显 popping 或统一降分辨率冒充结构性突破。
- 禁止把不完整 timestamp 当整帧，也禁止把频率不同的 DVFS 样本直接相减。
- 禁止恢复已失败的局部数组/动态索引 shader，除非有编译器寄存器证据和真机反证。
- 禁止把未运行的测试或静态截图写成运动/双眼视觉已验证。

## 测试结果

### 已执行并通过

- `cargo fmt --check`。
- `cargo test --all-targets`：37 passed，0 failed。
- `cargo check --target aarch64-linux-android --message-format=short`。
- `cargo check --no-default-features --all-targets --message-format=short`。
- `cargo check --no-default-features --target aarch64-linux-android --message-format=short`。
- `git diff --check`：通过，仅 Windows LF→CRLF 提示。
- PC Map_S03B 实际运行：39 assets、16 PointLight、96 estimated shadow views；WGSL 编译运行并完成截图。
- Android release + `render_debug` APK 构建、zipalign 校验、签名通过。
- APK 安装/增量安装到两台 A9210/PICO 设备，properties 与 HUD 生效。
- 最终设备 `PA9410MGJ9260457G` 已恢复并保持：
  - direct=1
  - shadows=1
  - hero=2
  - tail=2
  - dynamic overlay=1
  - shadow updates/frame=2
  - shadow Hz=8
  - HUD overview
- 最终 HUD 截图：`G:\zevy_engine\zevy_engine\target\render_debug\Pico2_Final_HUD.png`。

### 非阻塞警告

- 第三方 `bevy_mod_openxr` mismatched lifetime syntax。
- Cargo 同名 lib/bin PDB filename collision。
- 部分 glTF `TEXCOORD_2/3` 未被 Bevy 消费。
- APK 中 Map_S03B zip 与展开资产重复。

### 尚未完成

- 用户佩戴设备，在场景移动中验证 scalar 2H+2T 的灯光连续性、阴影方差和双眼一致性。
- 20～30 分钟 thermal soak、P95/P99、reprojection/missed frame。
- AGI/厂商 GPU capture 与 shader register/occupancy 数据。
- 解释/优化 16 shadow-enabled 对应当前仅 36～42 cache resident faces 的 view 分配；HUD 已先修正语义，仍需确认所有可见受光表面无距离 popping。
- 固定相机运动路径自动化；本阶段为固定起点静态采样。
- Shipping APK 真机性能基线；Shipping 编译组合已通过。

## 未完成步骤和下一步

1. 以当前 scalar 2H+2T 为 reference，建立最小 Cyclopean tile/froxel light-selection 数据路径。
2. 每 tile 共享 2 个 Hero、Tail CDF/reservoir 和双眼一致的 seed；片元只做局部 attenuation 修正与固定 H+K 次 shading。
3. 先做 direct-only 16/32/64 灯增长斜率 A/B，再接 shadow lookup；立即停止条件是 tile compute/barrier/寄存器成本不低于约 2.29 ms scan 节省，或出现双眼/深度边界伪影。
4. 并行补固定相机路径和 DVFS/thermal 记录，避免短窗口与频率变化误导。
5. 后续独立清理 578 MB APK 的重复 Map_S03B 资产，不与渲染算法 A/B 混在同一变量中。

唯一明确的下一步：**实现并真机 A/B 第一个双眼共享 tile 选灯原型，以消除逐片元 Hero/Tail 的两次 O(N) 候选扫描。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Checkpoints\2026-07-20-pico-ab-scalar-light-selection.md`
4. `G:\zevy_engine\zevy_engine\docs\VR_Renderring.md`，重点 20.2.1、20.2.2、20.4
5. `G:\zevy_engine\zevy_engine\src\config.rs`
6. `G:\zevy_engine\zevy_engine\src\app.rs`
7. `G:\zevy_engine\zevy_engine\src\scalable_lighting.rs`
8. `G:\zevy_engine\zevy_engine\src\shaders\zevy_pbr_functions.wgsl`
9. `G:\zevy_engine\zevy_engine\src\render_debug.rs`
10. `G:\zevy_engine\zevy_engine\src\scene.rs`
11. `G:\zevy_engine\zevy_engine\src\shadow_cache.rs`
12. 实际 `git status --short`、`git diff`、`git diff --cached` 与 branch/HEAD
