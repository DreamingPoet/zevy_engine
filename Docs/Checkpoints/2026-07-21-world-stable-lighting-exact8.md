# 阶段检查点：World-stable 多灯光与 exact 8 视觉基线

## 元数据

- 完成时间：2026-07-21 13:11，Asia/Shanghai
- 工作区：`G:\zevy_engine`
- 分支：`main`
- 阶段起始 HEAD：`08eced333d6698d84f74e2b47bf8d8ba6b4c93b9`
- 阶段提交：本文件与阶段代码同批提交；以包含本文件的实际 `main` HEAD 为准
- 目标设备：`PA9410MGJA190227G`（PICO A9210）
- 恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和阶段完成标准

最终目标仍是面向 VR 一体机的高性能、现代、大量动态灯光与动态阴影渲染器。允许修改 Bevy/wgpu/Naga/OpenXR/Vulkan 全栈，但不得用扩大 `light.range`、相机距离开关、删灯或突然关闭阴影换取性能。

本阶段要求：

1. 16 盏 Map_S03B PointLight 与阴影继续同时存在；
2. 消除 2x2 screen supercluster 在灯光交界和转头时产生、远处更明显的亮度块；
3. 消除未经重建的世界空间 stochastic shadow 斑块；
4. 左右眼、相机远近和头部旋转不改变已启用灯光/阴影的存在性；
5. 保留 scalar 与激进路径作可回退 A/B，并记录失败实验；
6. 在目标 PICO 上由用户佩戴裁决视觉结果。

阶段视觉门槛已通过：用户确认 `exact_lights=8` 时原转头亮度块没有回归，世界空间阴影斑块消失，画面正常。

## 已完成内容

### Zevy bevy_pbr fork 与 cluster ABI

- vendor `bevy_pbr 0.16.1` 到 `zevy_engine/third_party/crates/bevy_pbr-0.16.1`。
- Cargo `[patch.crates-io]` 让依赖图统一使用该 fork。
- storage cluster header 增加四个 PointLight ID 与四个 estimator weight，不增加 binding/render pass；Uniform 平台保持 fallback。
- 提供 cluster dimensions/AABB、PointLight entity list 和预选写入 API。

### 已证伪但保留的激进 2x2 screen supercluster

- 左右眼 union、2x2 XY cluster、CPU 2 Hero + 2 Tail 预选曾把 PICO Full GPU 从 30.29 ms 降至 23.78 ms。
- 用户佩戴发现灯光交界随转头出现屏幕块，且远处更明显。
- 数学原因：固定角宽 froxel 在深度 z 的世界宽度近似与 z 成正比，2x2 又扩大两倍；硬切 ID/权重随头部相对世界滑动。
- 该路径保留为 `world_reservoir=0, cluster_preselection=1, exact_lights=4` 的性能上界，不再是默认产品路径。

### 单遍 world-space reservoir

- 真实 cluster 一次遍历同时选 2 个确定性 Hero 与 2 路 weighted streaming reservoir，从 scalar reference 的约 2N importance scan 降为约 N。
- seed 不依赖屏幕、cluster、相机或眼睛；每片元一次 world seed，每候选一次 32-bit hash 并拆为两路 16-bit 随机数。
- sampled importance 在循环后重算，降低循环寄存器压力。
- 用户确认 screen-supercluster 转头亮度块消失。

### 已证伪的 raw stochastic shadow

- 用户截图显示地面/墙面出现固定世界空间的规则阴影斑块。
- 尺寸与 `floor(world_position * 8)` 的 12.5 cm cell 一致。
- 原因是把高方差 visibility 直接放进 `C_l/(K p_l)`；期望无偏不等于单帧 VR 可接受。
- 缩小 cell 只会变噪点，扩大 cell 只会变大斑块。未经重建的 raw stochastic shadow 禁止直出眼睛。

### 已验证的 exact local-list 保护

- 新配置 `RenderQualityConfig.point_light_exact_threshold`，默认 8、下限为 H+K、上限 64。
- shader 在所有近似路径之前检查真实 local list；`N <= 8` 时逐灯精确执行 BRDF、静态阴影与动态 overlay。
- Android 属性 `debug.zevy.exact_lights`：
  - 4：raw reservoir 性能/失败视觉档；
  - 6：仍有斑块的失败档；
  - 8：用户验证通过的默认档；
  - 16：当前地图全精确参考。
- HUD 显示 exact threshold，并区分 exact 与 overflow 的 BRDF 上限。
- 用户同一位置验证：6 仍有斑块，8 后斑块消失且画面正常。

## 当前文件和修改状态

阶段从 `main @ 08eced3` 的干净起点继续；本阶段与上一连续多灯阶段的改动将合并为一次提交。提交前修改包括：

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

没有发现额外无关用户代码改动。`RenderQualityConfig` 的默认 8 是用户测试后在磁盘中的实际值，已保留并同步到 shader、测试和文档。

## 关键决定与禁止事项

- `exact_lights=8` 是 Map_S03B 当前已验证视觉基线，不是 32/64 灯最终架构。
- 2x2 screen preselection 和 raw world shadow reservoir 都必须保留失败证据，禁止以后无条件恢复为默认。
- 8 灯以上 overflow 必须研究 stereo-shared reservoir/Top-K、edge-aware spatial reconstruction、短历史或低频 Tail proxy。
- 灯的物理 range、相机可见距离、shadow residency 和随机/重建质量必须解耦。
- 安装失败或设备掉线后立即通知用户；设备可用性由用户处理，不自行反复切换安装方案。
- PICO 系统截图可能暂停 NativeActivity，不能替代佩戴运动验收。

## 测试结果

### 自动与 PC

- `cargo fmt --all` / `cargo fmt --check`：通过。
- `cargo test --all-targets`：42 passed，0 failed。
- `cargo check --target aarch64-linux-android`：通过。
- `cargo check --no-default-features --all-targets`：通过。
- `cargo check --no-default-features --target aarch64-linux-android`：通过。
- PC Map_S03B 实际运行，Naga/WGSL 成功编译 `exact through 8 local lights`。
- 最终截图：`zevy_engine/target/render_debug/Map_S03B_exact8_pc.png`。
- Zevy 自有改动执行 `git diff --check`：通过，仅有 Windows LF->CRLF 提示；原样导入的上游 `bevy_pbr` vendor 保留其既有行尾空格，不据此改写第三方源码。

### Android/VR

- release + `render_debug` APK 构建、zipalign 和签名通过。
- profiling APK 安装到 `PA9410MGJA190227G`，ADB 返回 `Success`。
- 用户在该 APK 内比较 exact 6 与 8：6 失败、8 视觉通过。
- exact 6 非固定路径烟测曾约 CPU 8.8～9.7 ms、GPU 25.5～27.5 ms；不能外推为 exact 8 的固定 P95。
- 最终源码默认 8 的 APK 已重新构建但未重复安装，以免打扰已完成的用户视觉验证。

### 未完成/不能冒充通过

- exact 4/6/8/16 固定相机路径的 GPU P50/P95/P99 尚未完成。
- 每 cluster 真实 PointLight 数 telemetry 尚未补齐。
- 32/64 灯增长曲线、AGI capture、20～30 分钟 thermal soak 尚未完成。
- 8 灯以上 overflow reconstruction 尚未实现。

## 下一步

1. 补齐真实 max/avg lights per cluster 与 exact/overflow fragment telemetry。
2. 固定路径比较 exact 4/6/8/16 的性能与误差。
3. 实现双眼共享低分辨率 lighting/shadow reservoir、edge-aware reconstruction 或确定性 Top-K + Tail proxy。
4. 验证 16→32→64 灯成本斜率，并继续稀疏 dynamic shadow pages、Multiview 和 thermal soak。

唯一明确的下一步：**为 8 灯以上 overflow 建立可重建、双眼一致的固定成本路径；禁止再直接输出 raw stochastic shadow。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. 本文件
4. `G:\zevy_engine\zevy_engine\docs\VR_Renderring.md` 20.4.1～20.4.3
5. `G:\zevy_engine\zevy_engine\src\shaders\zevy_pbr_functions.wgsl`
6. `G:\zevy_engine\zevy_engine\src\config.rs`
7. `G:\zevy_engine\zevy_engine\src\scalable_lighting.rs`
8. `G:\zevy_engine\zevy_engine\src\clustered_light_preselection.rs`
9. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_pbr-0.16.1\ZEVY_FORK.md`
10. 实际 Git 状态、diff、branch/HEAD 与测试结果
