# 阶段检查点：PICO 4 Ultra Runtime interaction profile 修正

## 元数据

- 更新时间：2026-07-24 11:25，Asia/Shanghai
- 状态：本阶段代码、构建、安装与冷启动追踪验证已完成；物理按键和佩戴视觉验收待用户完成
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ 4d743fb665d5bd3440e49c48e150afebb281bc6a`
- 下一恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和完成标准

最终目标仍是建立面向 VR 一体机的高性能现代动态多灯光/阴影 renderer。本修正阶段的完成标准是：不依赖未经协商的 OpenXR 1.1 profile 名称；按目标 runtime 实际活动 profile 恢复控制器 pose/input 绑定；保持跨 runtime fallback 非致命；完成 Rust/Android 构建、APK 安装和 PICO 冷启动证据采集。

## 已完成内容

- [失败实验/用户实测] 把原有 `/interaction_profiles/bytedance/pico4s_controller` 无条件替换成 OpenXR 1.1 注册表中的 `pico4_controller` / `pico_neo3_controller` 后，目标设备完全不识别手柄且按键无响应。该方案已撤回，不能作为默认路径。
- [Android/VR 日志] PICO 4 Ultra Enterprise Runtime 2.2.0 在当前 OpenXR 1.0 会话中通过 `xrGetCurrentInteractionProfile` 报告 `/interaction_profiles/bytedance/pico4s_controller`；启用扩展日志中没有可用于无条件切换 promoted profile 的 `XR_BD_controller_interaction` 证据。
- [实现] `input.rs` 恢复原有 Oculus Touch、Valve Index、`pico4s_controller` 列表；相对 HEAD 不再有修改。
- [实现] `xr.rs` 的 grip pose 同时建议 Oculus Touch fallback 和目标 runtime 已验证的 `pico4s_controller`。支持列表按事件逐项提交，一个不支持 profile 的错误不会阻断后续 profile。
- [实现] vendored `bevy_mod_openxr` 的 `XR_ERROR_PATH_INVALID` / `XR_ERROR_PATH_UNSUPPORTED` / action-set 错误包含具体 interaction profile，便于区分 fallback 失败和活动 profile 失败。
- [Android/VR 日志] 修正 APK 冷启动进入 `FOCUSED`；Map_S03B 加载；左右 grip 均获得有效 tracking flags，并分别生成 fixed `DynamicOverlay` 球和 fixed `FullyDynamic` shadowed SpotLight。
- [Android/VR 日志] 当前 runtime 明确拒绝 Oculus Touch grip fallback，但随后接受 `pico4s_controller`，action sync 正常；没有 panic、fatal shader error 或 Vulkan validation fatal。

## 当前文件和修改状态

工作区没有 staged/commit。XR hand harness 开始前 worktree clean；以下是仍未提交的本阶段组合改动：

```text
 M Docs/Checkpoints/CURRENT.md
 M zevy_engine/docs/VR_Renderring.md
 M zevy_engine/src/app.rs
 M zevy_engine/src/scene.rs
 M zevy_engine/src/shadow_motion_policy.rs
 M zevy_engine/src/shadow_overlay.rs
 M zevy_engine/src/xr.rs
 M zevy_engine/third_party/crates/bevy_mod_openxr-0.3.0/src/openxr/action_binding.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/light.rs
?? Docs/Checkpoints/2026-07-24-map-s03b-xr-hand-shadow-harness.md
?? Docs/Checkpoints/2026-07-24-pico4s-runtime-binding-correction.md
?? zevy_engine/src/scene/map_s03b_xr_hand_test.rs
```

`zevy_engine/target/release/apk/zevy_engine.apk` 是 ignored 构建产物。没有发现独立的用户未提交代码改动；不要把整个工作区回退到 HEAD，否则会丢失 XR hand harness、SpotLight policy 和动态 caster shadow queue 修复。

## 关键决定与禁止事项

- OpenXR 注册表中的标准名称不等于当前 runtime 已支持、已启用或已激活的名称；目标设备的扩展/API 能力与 `xrGetCurrentInteractionProfile` 证据优先。
- `pico4s_controller` 是当前 PICO 4 Ultra Runtime 2.2.0 的活动 profile，不得因 OpenXR 1.1 注册表命名再次删除。
- Oculus compatibility/fallback 可以保留，但单个 `XR_ERROR_PATH_UNSUPPORTED` 必须非致命；禁止让 fallback 失败阻断 PICO 原生绑定。
- 将来启用 `pico4_controller` / `pico_neo3_controller` 前必须先协商 API 版本或对应扩展，并保留 runtime-proven fallback。
- 手电筒当前继续使用 grip pose；若朝向不自然，改用独立 `/input/aim/pose`，禁止写 PICO 专属欧拉角补丁。
- 本修正不引入第二套 XR 插件，不改变 VR Camera、XR 起点、灯光 range 或 shadow policy。

## 测试结果

- `cargo fmt`：通过。
- `cargo test --lib`：63 passed，0 failed。
- `cargo check --target aarch64-linux-android --message-format=short`：通过。
- `cargo check --no-default-features --target aarch64-linux-android --message-format=short`：通过。
- release + `render_debug` APK 构建、4K alignment、v3 签名：通过。
- APK：737,278,005 bytes，2026-07-24 11:17:44，SHA-256 `E4AD277D86C73407F36E366065F1BA2460D305086CED91BBB42BB13A14A59157`。
- 设备 `PA9410MGJ9260457G`：`adb install -r` 成功；冷启动 PID `9217`，OpenXR `FOCUSED`，Map_S03B 与左右 grip tracking 正常。
- 未执行：用户实际按 A/B、摇杆、扳机；手电筒 aim 方向；动态阴影视觉、tracking loss、双眼一致；GPU/thermal 测量。
- 唯一编译 warning 是既有 `bevy_mod_openxr` elided lifetime 提示。

## 未完成步骤和下一步

1. 用户佩戴当前 APK，实际验证 A/B、摇杆、扳机响应，以及左球和右灯跟手。
2. 验证左球动态投影、tracking loss 清理和双眼一致。
3. 若按键仍失败，在按键期间记录每个 OpenXR action state 与 active profile；禁止继续仅凭名称猜测。
4. 若仅光束朝向不自然，新增 right aim Action Space 后重测。
5. 视觉正确后测 SpotLight 增量 GPU P50/P95/P99 与 thermal；第二独立 fixture 仍是通用化验收要求。

唯一明确的下一步：**用户在已安装的 11:17:44 APK 中实际按下右手 A/B、摇杆和扳机，并确认左球与右灯是否可见、跟随和朝向正确。**

## 恢复时首先读取

1. `AGENTS.md`
2. `Docs/Checkpoints/CURRENT.md`
3. `Docs/Checkpoints/2026-07-24-pico4s-runtime-binding-correction.md`
4. `zevy_engine/src/input.rs`
5. `zevy_engine/src/xr.rs`
6. `zevy_engine/src/scene/map_s03b_xr_hand_test.rs`
7. `zevy_engine/third_party/crates/bevy_mod_openxr-0.3.0/src/openxr/action_binding.rs`
8. `Docs/Design/Shadow_Motion_Policies.md`
9. 实际 Git、测试、APK 与 ADB/logcat 状态
