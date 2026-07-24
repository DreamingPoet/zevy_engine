# 阶段检查点：Map_S03B XR 双手动态阴影验证夹具

## 元数据

- 更新时间：2026-07-24 09:41，Asia/Shanghai
- 状态：实现、Rust 测试、Android 编译与 APK 完成；ADB 无设备，VR 验收待完成
- 工作区：`G:\zevy_engine`
- 分支 / HEAD 基线：`main @ 4d743fb665d5bd3440e49c48e150afebb281bc6a`
- 恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和完成标准

最终目标是面向 VR 一体机的高性能现代动态多灯光/阴影 renderer。本阶段不是为 Map_S03B 写渲染特例，而是用真实双手运动验证通用 motion policy：左手生成固定 `DynamicOverlay` caster，右手生成固定 `FullyDynamic` shadowed SpotLight，失去有效追踪后清理，并保持左右眼共享。

选择 SpotLight 的数学理由是单 shadow frustum 的 caster 成本近似 \(O(S+D)\)，而 FullyDynamic PointLight cubemap 约为 \(O(6(S+D))\)。这只是成本模型；本阶段没有 PICO GPU 实测，不能宣称已获得性能收益。

## 已完成内容

- [实现] 使用现有 OpenXR/XR 插件创建左右 grip Action Space，读取真实 `XrSpaceLocationFlags`，并继承 `XrTrackingRoot`。
- [实现] 统一 Oculus Touch、Valve Index、标准 PICO 4/PICO Neo3 profile；同时修正输入模块原有非标准 PICO profile 字符串。
- [实现] 新增 `scene/map_s03b_xr_hand_test.rs`：左手 0.22 m PBR 球固定 `DynamicOverlay`；右手 120,000 lm、20 m、16°/28° shadowed SpotLight 固定 `FullyDynamic`。
- [实现] tracking loss、Map 切换和 XR session teardown 清理运行时实体；调试 anchor 不投射/接收阴影。
- [实现] SpotLight 接入通用 light motion policy；P1 无 spot cache，因此显式走真实 FullyDynamic 单 frustum。
- [实现] 动态 caster 只从缓存 PointLight static layer 排除，随后重新加入 Spot/Directional 原生阴影 phase。
- [实现] 修复 Bevy shadow phase 先验证 cache、后检查 caster flag 导致 Static→Dynamic 迁移可能残留旧影的问题。
- [PC/编译] 全量 63 项 Rust 测试通过。
- [Android 构建] arm64 check、release/render-debug APK、4K alignment、v3 签名通过。
- [未验证] ADB 列表为空，未安装、未启动、未进行 PICO 佩戴测试。

## 当前文件和修改状态

本阶段开始时 worktree clean；当前改动全部来自本阶段，未暂存、未提交：

```text
 M zevy_engine/src/app.rs
 M zevy_engine/src/input.rs
 M zevy_engine/src/scene.rs
 M zevy_engine/src/shadow_motion_policy.rs
 M zevy_engine/src/shadow_overlay.rs
 M zevy_engine/src/xr.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/light.rs
?? zevy_engine/src/scene/map_s03b_xr_hand_test.rs
 M Docs/Checkpoints/CURRENT.md
?? Docs/Checkpoints/2026-07-24-map-s03b-xr-hand-shadow-harness.md
 M zevy_engine/docs/VR_Renderring.md
```

APK 位于 ignored `zevy_engine/target/release/apk/zevy_engine.apk`，737,273,909 bytes，2026-07-24 09:40:02。
SHA-256：`FD5845B50B5F626D8ADA9BF45BD6B4C27E87F5734C6CA62879B7A1A4CAAC41A5`。

## 关键决定与禁止事项

- Map 名称只控制测试内容是否生成，不进入 renderer policy 或 shadow queue 算法。
- 两个 policy 都是 fixed；手暂时静止不能导致降级。
- 不扩大 `light.range` 处理可见性，不使用相机距离突变，不做 per-eye policy。
- 手柄追踪无效时禁止使用旧 Transform。
- 当前射灯跟随 grip pose local `-Z`；若真机方向不自然，必须实现标准 aim pose，不写设备专属旋转补丁。
- PICO profile 在当前 OpenXR 1.0 runtime 上是否原生接受必须由真机日志确认；保留已工作的 Oculus Touch compatibility binding。
- 不新增 XR 插件，不改变 VR Camera、XR 起点或 UE 导入格式。

## 测试结果

- `cargo fmt`：通过。
- `cargo test --lib`：63 passed，0 failed。
- `cargo check --target aarch64-linux-android --message-format=short`：通过。
- `cargo check --no-default-features --target aarch64-linux-android --message-format=short`：Shipping feature 组合通过。
- APK build、4K alignment、v3 signing：通过。
- `adb devices -l`：无设备；未安装，未进行 Android/VR 视觉或性能验证。
- 既有 warning：`bevy_mod_openxr` elided lifetime；没有新增编译错误或测试失败。

## 未完成步骤和下一步

1. PICO 恢复连接后安装现有 APK；安装失败立即通知用户。
2. 冷启动检查 OpenXR profile、tracked flags、panic/shader error。
3. 验证左球跟随、右灯朝向、球对墙地投影、tracking loss 清理和双眼一致。
4. grip 朝向不正确时实现 right aim Action Space 后重测。
5. 视觉通过后采集 SpotLight 增量 GPU 成本；第二独立 fixture 仍是通用化验收要求。

唯一下一步：**设备恢复后安装本阶段 APK并完成 XR 佩戴验证。**

## 恢复时首先读取

1. `AGENTS.md`
2. `Docs/Checkpoints/CURRENT.md`
3. `Docs/Design/Shadow_Motion_Policies.md`
4. `zevy_engine/src/scene/map_s03b_xr_hand_test.rs`
5. `zevy_engine/src/xr.rs`
6. `zevy_engine/src/shadow_motion_policy.rs`
7. `zevy_engine/src/shadow_overlay.rs`
8. vendored `bevy_pbr` `render/light.rs`
9. 实际 Git、测试、APK 和 ADB 状态
