# 任务检查点：Shadow Motion Lab 16→32→64 与 Pico 成本分解

## 元数据

- 更新时间：2026-07-24，Asia/Shanghai
- 状态：阶段完成；长时 thermal/GPU capture 留给后续性能验收
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ 0aa98d12f554ef8e92323b4b3b962641797b5087`
- 阶段代码状态：未提交，与 P2 sparse cross-fade 连续存在于同一 dirty worktree

## 最终目标和完成标准

最终目标是面向 VR 一体机的高性能现代动态多灯光/阴影 renderer。当前阶段用独立于 Map_S03B 的确定性 16/32/64 灯 fixture，证明 P2 SlowMoving cross-fade、DynamicOverlay、direct-light overflow 的正确性和真机成本斜率，并把下一瓶颈定位到具体成本项。

阶段完成标准为：固定除灯数外的所有输入；PC/Android 均主动触发 transition；左右眼共享状态；记录 16→32→64 整帧成本；在 16 灯用 geometry/direct/shadow/full 单变量分解主导项；失败输出必须归因且保留 reference。

## 已完成内容

- [实现] 新增通用 `ShadowMotionLab`、嵌套 16/32/64 profile、确定性 SlowMoving 灯和 4 个 DynamicOverlay caster。
- [实现] profiling-only `debug.zevy.level`，Shipping 不读取测试 Level override。
- [实现] 修复显式 Static runtime caster 被错误送入 dynamic overlay 的通用分类问题。
- [实现] 新增 `scripts/profile_shadow_motion_lab.ps1`，固化冷启动、四档开关、warmup、PxrMetric 解析、频率筛选和 P95。
- [实现] 从默认 XR 产品路径移除 `HandGizmosPlugin`；不删除 hand tracking、左手球或右手电筒 harness。
- [PC] 16/32/64 分别得到 96/192/384 resident views，4 个 transition 全部活跃；64 raw world-reservoir 出现棋盘斑块，全精确 64 消除，归因到 direct-light overflow 而非 shadow cross-fade。
- [Android/VR，设备 `PA9410MGJ9260457G`] 三档均主动触发 transition，左右眼 HUD 状态一致，无 panic/wgpu/Vulkan validation failure。

Android 主实验：

| 灯数 | active / wait | max stale | FPS avg | GPU avg / P95 |
|---:|---:|---:|---:|---:|
| 16 | 4 / 12 | 0.026 m | 17.65 | 53.47 / 58.51 ms |
| 32 | 4 / 28 | 0.046 m | 11.10 | ≥66.67 / ≥66.67 ms |
| 64 | 4 / 60 | 0.074 m | 7.75 | ≥66.67 / ≥66.67 ms |

32/64 达到 PxrMetric 66.67 ms 上限，该值是下界而非真实耗时。

16 灯四档：

| 路径 | GPU avg / P95 |
|---|---:|
| Geometry/post | 13.62 / 14.04 ms @456 MHz |
| Direct only | 35.18 / 39.60 ms @599 MHz |
| Shadow submission only | 12.60 / 13.13 ms @456 MHz |
| Full | 52.07 / 55.14 ms @599 MHz |

固定 preselection 后完整 shadowed sample 下界：4/2/1 样本分别约 51.55/41.36/31.49 ms GPU。由此证伪“只靠 Top-K 可达到 72 Hz”；下一结构目标必须同时降低 $K$ 与昂贵片元基数 $P$。

## 当前文件和修改状态

- 分支/HEAD 未变化，无 staged、无 commit。
- 本阶段主要文件：`Docs/Design/Shadow_Motion_Lab.md`、`Docs/render_debug.md`、`src/scene/shadow_motion_lab.rs`、`src/scene.rs`、`src/platform.rs`、`src/render_debug.rs`、`src/app.rs`、`scripts/profile_shadow_motion_lab.ps1`。
- P2 的 config、shadow policy/cache/overlay、shader 和 vendored `bevy_pbr` ABI 修改仍未提交且与本阶段相互依赖，不得部分回退。
- `src/config.rs` 的 `xr_render_scale: 1.0` 是用户明确保留的已有配置，不得改回 0.8。
- `target/shadow_motion_lab/**`、APK 和截图均为 ignored 测试工件。

## 关键决定与禁止事项

- Map_S03B 和 ShadowMotionLab 都只是 fixture；renderer 不得按 Level/Actor/固定坐标分支。
- 灯的物理 range 与相机可见/驻留分离，不靠扩大 range 或距离关闭灯作弊。
- raw screen/world stochastic block 已被视觉证伪，不能直接输出到眼睛。
- 4 个 sparse transition slot 的理论吞吐固定，wait/stale 随灯数增长是模型结果；后续要做优先级/deadline 或新表示，不能假装所有灯保持 8 Hz。
- 真机数据说明 stable cache 下主导项是 full-resolution direct lighting + shadow sampling，不是简单的 cubemap submission 数。
- 未经用户明确要求不提交；设备安装失败立即通知用户，不代替用户长时间排查连接。

## 测试结果

- `cargo fmt --all -- --check`：通过。
- `cargo test --lib`：阶段末 73/73 通过（下一阶段新增测试后为 74）。
- Android default/no-default feature checks：通过。
- release profiling APK：构建、4K alignment、签名、安装和本阶段三档/四档测试成功。
- 未执行：固定 camera path 60 秒 16→32→64→32→16 回程、GPU capture、误差图、20～30 分钟 thermal soak。

## 未完成步骤和下一步

1. 用 Forward/Deferred/reduced-rate 三条可回退路径攻击片元项 $P$。
2. 为 8 灯以上 overflow 实现无块状输出的双眼共享 Top-K/重建，而不是 raw reservoir。
3. 后续单独处理 DynamicOverlay face 随灯数近线性增长和 sparse transition deadline。

唯一明确下一步：实现并测量全分辨率 DeferredReference substrate，同时探测目标 runtime 的 foveation/FDM 能力，再进入 low-resolution local-lighting + edge-aware reconstruction。

## 恢复时首先读取

- `AGENTS.md`
- `Docs/Checkpoints/CURRENT.md`
- `Docs/Design/Shadow_Motion_Lab.md`
- `Docs/Design/Reduced_Rate_Local_Lighting.md`
- `zevy_engine/src/config.rs`
- `zevy_engine/src/platform.rs`
- `zevy_engine/scripts/profile_shadow_motion_lab.ps1`

