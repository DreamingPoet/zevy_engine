# 阶段检查点：XR 手柄动态灯光/阴影验证夹具完成

## 元数据

- 更新时间：2026-07-24 13:02，Asia/Shanghai
- 状态：实现与 Rust 测试完成，准备阶段提交；最新左手调试盒隐藏尚未重新安装真机
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：阶段基线 `main @ 4d743fb665d5bd3440e49c48e150afebb281bc6a`；本文件随阶段提交，实际提交以 Git HEAD 为准
- 下一恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和完成标准

最终目标是建立面向 VR 一体机的高性能现代动态多灯光/阴影 renderer。本阶段以真实 OpenXR 手柄运动验证通用 motion policy：左手生成 fixed `DynamicOverlay` caster，右手生成 fixed `FullyDynamic` shadowed SpotLight，失去追踪后清理；动态 caster 正确进入 PointLight overlay 与 Spot/Directional 原生实时阴影，同时不污染持久化 PointLight static cache。

阶段完成标准包括：通用 policy/queue 机制实现、Map_S03B 只作 harness、PICO runtime profile 按实机证据纠正、Rust/Android 构建验证、真实 grip tracking 验证，以及未完成的佩戴视觉/GPU 项明确留档。

## 已完成内容

- [实现] 直接 OpenXR grip Action Space 暴露真实 position/rotation tracking flags，并继承唯一 `XrTrackingRoot`。
- [实现] 左手有效 pose 生成半径 `0.04 m`、`ico(5)` 的 PBR 球，固定 `ShadowCasterMotionPolicy::DynamicOverlay`；tracking loss 立即移除。
- [实现] 左 grip anchor 改为不可见的纯追踪父节点，移除其 `Mesh3d`/`MeshMaterial3d`；没有用会隐藏子球的父级 `Visibility::Hidden`。右 grip 调试盒保留。
- [实现] 右手有效 position+rotation 生成 `520,000 lm`、`30 m`、`16°/28°` 的 shadowed SpotLight，固定 `LightShadowMotionPolicy::FullyDynamic`。
- [实现] SpotLight 接入通用 motion policy；动态 caster 在 PointLight static queue 中排除，但重新加入 Spot/Directional 原生 shadow phase；修复 Static→Dynamic 后旧 phase cache 残影风险。
- [实现/Android 日志] PICO 4 Ultra Enterprise Runtime 2.2.0 实际活动 profile 为 `/interaction_profiles/bytedance/pico4s_controller`。恢复该输入路径，grip 同时保留 Oculus fallback；单个不支持 profile 非致命，错误日志包含具体 profile。
- [Android/VR 日志] 上一版 APK 安装并进入 OpenXR `FOCUSED`，Map_S03B 加载，左右 grip tracking flags 有效，左球和右 SpotLight 均创建，无 panic/fatal shader error。
- [文档] `VR_Renderring.md` 记录 interaction profile 失败实验、runtime 证据和 capability-driven binding 原则。

## 当前文件和修改状态

本阶段提交包含：

```text
Docs/Checkpoints/CURRENT.md
Docs/Checkpoints/2026-07-24-map-s03b-xr-hand-shadow-harness.md
Docs/Checkpoints/2026-07-24-pico4s-runtime-binding-correction.md
Docs/Checkpoints/2026-07-24-xr-hand-shadow-harness-complete.md
zevy_engine/docs/VR_Renderring.md
zevy_engine/src/app.rs
zevy_engine/src/scene.rs
zevy_engine/src/scene/map_s03b_xr_hand_test.rs
zevy_engine/src/shadow_motion_policy.rs
zevy_engine/src/shadow_overlay.rs
zevy_engine/src/xr.rs
zevy_engine/third_party/crates/bevy_mod_openxr-0.3.0/src/openxr/action_binding.rs
zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/light.rs
```

用户另有 `zevy_engine/src/config.rs` 的 `xr_render_scale 0.8 -> 1.0` 调参，本次明确不暂存、不提交，提交后仍应保留为工作区修改。APK 位于 ignored `target/`，不提交。

## 关键决定与禁止事项

- Map_S03B 只决定测试内容是否生成，不进入 renderer policy/queue 算法。
- fixed/manual policy 不因手暂时停止而降级；tracking 无效时禁止使用 stale pose。
- 左右眼共享同一 pose、灯、caster 和 policy；禁止 per-eye 状态。
- 灯光物理 range 与相机可见性继续分离。
- interaction profile 必须由 API/扩展协商和 runtime 活动 profile 裁决；不得再次仅凭 OpenXR 1.1 注册表删除 `pico4s_controller`。
- 若手电筒方向不自然，使用独立 `/input/aim/pose`，禁止 PICO 专属旋转常量。
- 不覆盖或混入用户的 `xr_render_scale = 1.0` 调参。

## 测试结果

- `cargo fmt`：通过。
- `cargo test --lib`：63 passed，0 failed；在左调试盒隐藏后再次通过。
- 隐藏前完整阶段：Android default/no-default arm64 checks 通过；release/render-debug APK、4K alignment、v3 签名通过。
- 隐藏前 APK：设备 `PA9410MGJ9260457G` 安装成功，OpenXR `FOCUSED`，活动 `pico4s_controller`，左右 grip tracked。
- 最新隐藏改动：APK 构建等待被用户中断，不能记为已确认构建或已安装；必须从提交后的 HEAD 重建。
- 未执行：物理 A/B/摇杆/扳机验收、左球/右灯佩戴视觉、tracking loss、双眼一致、SpotLight GPU P50/P95/P99 与 thermal soak、第二独立 fixture。

## 未完成步骤和下一步

1. 从本阶段提交 HEAD 构建并安装 APK，确认左 grip 调试盒消失，4 cm 球仍可见且投影。
2. 验证 A/B、摇杆、扳机，以及左球跟手、右灯方向、tracking loss 和双眼一致。
3. 若按键失败，抓取物理按键期间 action state；若仅方向错误，新增 aim Action Space。
4. 视觉通过后测 SpotLight 增量 GPU/thermal 成本，并在第二 fixture 验证通用性。

唯一明确的下一步：**从新提交构建并安装 APK，先验证隐藏调试盒没有连带隐藏 DynamicOverlay 球。**

## 恢复时首先读取

1. `AGENTS.md`
2. `Docs/Checkpoints/CURRENT.md`
3. `Docs/Design/Shadow_Motion_Policies.md`
4. `zevy_engine/src/scene/map_s03b_xr_hand_test.rs`
5. `zevy_engine/src/xr.rs`
6. `zevy_engine/src/input.rs`
7. `zevy_engine/src/shadow_motion_policy.rs`
8. `zevy_engine/src/shadow_overlay.rs`
9. 两个 vendored `action_binding.rs` / `bevy_pbr render/light.rs`
10. 实际 Git、测试、APK 与 ADB 状态
