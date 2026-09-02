# 任务检查点：Shadow Motion Policy P2 稀疏双快照阴影

## 元数据

- 更新时间：2026-07-24 14:00，Asia/Shanghai
- 状态：实现完成；PC 主动路径和 Android/VR 管线已验证，Android 活动 SlowMoving 视觉验收待后续压力夹具
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ 0aa98d12f554ef8e92323b4b3b962641797b5087`
- 下一恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和完成标准

最终目标是建立面向 VR 一体机的、高性能、现代、支持大量动态灯光与动态阴影的 Zevy renderer。本阶段把 `SlowMoving` PointLight 从“Transform 变化就重画六面”的路径改为关键帧静态 cubemap、稀疏旧快照和双眼共享 visibility cross-fade，同时保留真实灯位的动态 caster overlay。

完成标准：机制必须通用；配置和设备容量可裁剪；不扩大物理 `light.range`；左右眼共享状态；dynamic caster 不被静态快照冻结；通过 PC、Rust、Android 编译、APK 和 Pico 启动审计。Map_S03B 只作测试夹具。

成本模型由

\[
C_{old}=O\bigl(6(S+D)\bigr)
\]

变为

\[
C_{P2}=O\bigl(U_k\,6S\bigr)+O\bigl(6D\bigr)+O(P_t),\qquad U_k\ll1,
\]

其中额外旧快照采样只发生在活动 transition 的灯上。

## 已完成内容

- [实现] vendored `bevy_pbr` 新增 `PointLightShadowMapTransition`，并扩展 CPU/GPU/WGSL clusterable-light ABI 至 112 bytes；16 KiB uniform 对象上限同步改为 146，并有尺寸断言。
- [实现] 主世界 SlowMoving scheduler 支持 4 cm / 8 Hz 关键帧、0.12 s blend、near-Z 强制重画、stale-first 确定性槽竞争、槽完成释放和显式 cache invalidation。
- [实现] atlas 布局为 `[N static][N dynamic][K_eff previous]`，其中 `K_eff=min(K_config,max(0,floor(L_max/6)-2N))`；旧 cubemap 一次复制六层，设备不足先缩 transition pool。
- [实现] render node 使用跨眼原子 claim，保证旧快照只复制一次；RenderWorld 把设备实际槽数反馈给主世界。
- [实现] shader 分别以当前/旧快照原点采样静态阴影并平滑混合，再乘以真实物理灯位的 dynamic overlay。
- [实现] 新增配置、active/start/wait/copy/effective-slots/max-stale HUD 与文档。
- [PC] 普通 Map 路径正常；临时 SlowMoving 主动夹具达到 active `2/4`，无 shader/wgpu/Vulkan 校验错误。临时 Map 源码已恢复且无 diff。
- [Android/VR] A9210 / PICO 4 Ultra Enterprise `PA9410MGJ9260457G` 上 release profiling APK 构建、签名、覆盖安装与 OpenXR 1.1 冷启动通过。
- [Android/VR] 最终源码保留用户 `xr_render_scale=1.0`；日志确认双眼 1920x1920、MSAA 2x，截图 HUD 左右眼均为 `Slow shadow xfade 0/4`，说明设备提供 4 个有效槽且双眼状态一致。

## 当前文件和修改状态

- 分支/HEAD 未改变；没有 staged 文件，没有创建 commit。
- P2 修改集中于：
  - `zevy_engine/src/config.rs`
  - `zevy_engine/src/render_debug.rs`
  - `zevy_engine/src/scalable_lighting.rs`
  - `zevy_engine/src/shaders/zevy_pbr_functions.wgsl`
  - `zevy_engine/src/shadow_cache.rs`
  - `zevy_engine/src/shadow_motion_policy.rs`
  - `zevy_engine/src/shadow_overlay.rs`
  - `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/cluster/mod.rs`
  - `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/lib.rs`
  - `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/light/mod.rs`
  - `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/light/point_light.rs`
  - `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/light.rs`
  - `zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/mesh_view_types.wgsl`
  - `Docs/Design/Shadow_Motion_Policies.md`
  - `Docs/render_debug.md`
  - `zevy_engine/docs/VR_Renderring.md`
  - `Docs/Checkpoints/CURRENT.md` 与本快照。
- `zevy_engine/src/config.rs` 同时含用户原有 `xr_render_scale: 1.0` 与 P2 改动。未来提交时必须明确其归属；不得覆盖用户设置。
- `zevy_engine/target/**` 下 PC/Pico 截图和 APK 是 ignored 工件。

## 关键决定与禁止事项

- 物理灯位、当前静态快照位和旧快照位是三个不同概念；dynamic caster 只使用物理灯位。
- transition pool 必须稀疏，禁止为每灯永久分配第三 cubemap。
- 主世界产生唯一 transition 状态，左右眼共享；禁止 per-eye 槽、时间或随机状态。
- near-Z 变化的深度编码不可混合，必须直接失效重画。
- 无活动 transition 的灯不能支付旧 shadow sample；未启用阴影的 SlowMoving 灯不占槽。
- 不能用扩大 `light.range`、按相机距离关灯/关影或 Map 专属硬编码解决 residency/性能问题。
- 13.6 FPS 真机截图不是 P2 A/B 结论：它来自 1.0 scale、exact-through-18 和非固定头姿的瞬时数据。
- 未经用户明确要求不得提交。

## 测试结果

- `cargo fmt --all -- --check`：通过。
- `cargo test --lib`：68 passed，0 failed；最终 1.0-scale 源码再次通过。
- 稀疏槽竞争定向测试连续 5 次：通过。
- Android default-feature 与 `--no-default-features` 两种 `cargo check`：通过。
- `cargo apk build --lib --release`：release profiling APK 构建、alignment verification 和签名通过。
- ADB 覆盖安装、OpenXR 冷启动、进程存活与双眼截图：通过。
- PC 主动 transition：通过；Pico 当前 Map active=0，因为自动分类中没有 SlowMoving 灯。
- 未执行：Android 活动 transition 的佩戴式视觉验收、固定相机 P50/P95/P99、GPU capture、误差图、16→32→64 压力曲线、20–30 分钟 thermal soak、第二独立场景。
- 既有 `bevy_mod_openxr` mismatched lifetime syntax warning 仍存在，与本阶段无关。

## 未完成步骤和下一步

1. 建立不依赖 Map 名称的 Shadow Motion Policy 合成压力夹具，覆盖 16/32/64 个 SlowMoving 灯、动态 caster、不同速度/尺度和 transition 槽超订阅。
2. 在 Pico 上主动触发并佩戴检查 cross-fade，记录 active/wait/copy/max-stale、GPU ms 和误差图。
3. 在实测上实现 P2b：Hero/交互灯优先级、最大 stale deadline、GPU-ms 预算和可抢占调度。
4. 若第二 shadow sample 的 fragment 成本不合格，再研究 tile/receiver 限定或 temporal reconstruction，不直接输出随机阴影。

唯一明确下一步：**实现通用合成压力夹具，先完成 Pico 16/32/64 SlowMoving 灯主动验证，再开发 P2b 调度。**

## 恢复时首先读取

- `G:\zevy_engine\AGENTS.md`
- `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
- `G:\zevy_engine\Docs\Design\Shadow_Motion_Policies.md`
- `G:\zevy_engine\zevy_engine\docs\VR_Renderring.md`
- `G:\zevy_engine\zevy_engine\src\shadow_motion_policy.rs`
- `G:\zevy_engine\zevy_engine\src\shadow_cache.rs`
- `G:\zevy_engine\zevy_engine\src\shadow_overlay.rs`
- `G:\zevy_engine\zevy_engine\src\shaders\zevy_pbr_functions.wgsl`
- 实际 Git 状态、diff、branch/HEAD 与最新测试。
