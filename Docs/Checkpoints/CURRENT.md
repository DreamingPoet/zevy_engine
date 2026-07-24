# 当前任务检查点：Map_S03B XR 双手动态阴影验证夹具

## 元数据

- 更新时间：2026-07-24 13:02，Asia/Shanghai
- 状态：XR hand harness、运行时 profile 修正和左手调试盒隐藏已实现；63 项 Rust 测试通过；profile/grip 已在上一版 APK 真机验证，最新隐藏改动尚未重新安装验收
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：阶段基线为 `main @ 4d743fb665d5bd3440e49c48e150afebb281bc6a`；本检查点随阶段提交，恢复时以实际 `git rev-parse HEAD` 为准
- 本阶段历史快照：`Docs/Checkpoints/2026-07-24-xr-hand-shadow-harness-complete.md`
- Profile 修正快照：`Docs/Checkpoints/2026-07-24-pico4s-runtime-binding-correction.md`
- 失败前状态快照：`Docs/Checkpoints/2026-07-24-map-s03b-xr-hand-shadow-harness.md`
- 上一阶段：`Docs/Checkpoints/2026-07-23-shadow-motion-policy-p1.md`
- 下一恢复入口：本文件

## 最终目标和完成标准

最终目标仍是建立面向 VR 一体机的、高性能、现代、支持大量动态灯光与动态阴影的 Zevy renderer。本阶段用真实 XR 手柄运动为通用 `LightShadowMotionPolicy` / `ShadowCasterMotionPolicy` 增加可观察的 Map_S03B 测试夹具，不把 Map 名称、坐标或固定灯数写进 renderer 算法。

本阶段完成标准：

1. 左手位置有效时生成并跟随一个固定 `DynamicOverlay` 球；失去位置追踪后立即移除。
2. 右手位置和方向都有效时生成并跟随一个固定 `FullyDynamic` 有阴影射灯；任一追踪无效后立即移除。
3. 两个对象继承 `XrTrackingRoot`，因此共享 Map_S03B 的 XR 起点与玩家移动。
4. 动态球进入缓存点光源的动态层以及射灯/方向光的原生实时阴影，不污染持久化点光源静态层。
5. 通过 Rust 全量测试、Android arm64 检查和 APK 构建；PICO 上的追踪方向、阴影、双眼一致和性能必须明确留待真机裁决。

成本模型：阴影射灯只有一个 shadow frustum，近似为

\[
C_{spot}\approx O(S+D),
\]

而同等 FullyDynamic PointLight cubemap 近似为 \(O(6(S+D))\)。左手球作为动态 overlay 只向可见点光源 face 提交自身几何，静态场景不会因此每帧重画；主画面的逐片元射灯成本仍需在 PICO GPU 上测量。

## 已完成内容

### [实现] XR 追踪与生命周期

- `xr.rs` 使用现有 `bevy_mod_openxr`、`bevy_mod_xr` 和 `bevy_xr_utils`，没有引入新 XR 插件。
- 为左右 grip pose 创建直接 OpenXR Action Space，并暴露 `XrSpaceLocationFlags`，避免把失去追踪后的旧 Transform 当成有效手位。
- Action Space 带 `XrTracker`，由现有关系钩子自动成为 `XrTrackingRoot` 子级。
- 纠正 interaction profile 策略：目标 PICO 4 Ultra Enterprise Runtime 2.2.0 通过 `xrGetCurrentInteractionProfile` 实际激活 `/interaction_profiles/bytedance/pico4s_controller`。输入动作恢复 Oculus Touch、Valve Index、`pico4s_controller` 的原有列表；grip pose 同时建议 Oculus Touch fallback 与运行时已验证的 `pico4s_controller`。
- OpenXR 1.1 注册表中的 `/interaction_profiles/bytedance/pico4_controller` 与 `pico_neo3_controller` 不能在当前 OpenXR 1.0 会话中无条件替换运行时活动路径。未来只有在 API/扩展能力协商成功后才启用这些 promoted profile。
- vendored `bevy_mod_openxr` 的绑定错误现在记录具体 interaction profile，避免把一个非致命 fallback 失败误判为所有手柄绑定失败。
- XR session 销毁前清理 Zevy anchor；头/手调试几何标记为 `NotShadowCaster` / `NotShadowReceiver`，不会意外进入动态阴影成本。

### [实现] Map_S03B 测试夹具

- 新增 `scene/map_s03b_xr_hand_test.rs`，只负责生成测试内容，不修改导入 Level 数据，也不实现 renderer 特例。
- 左手：半径 `0.04 m` 的蓝色 PBR 球，固定 `ShadowCasterMotionPolicy::DynamicOverlay`，作为左 grip anchor 子实体。
- 左 grip anchor 现在是不可见的纯 Action Space/追踪父节点，不再携带调试 `Mesh3d` / `MeshMaterial3d`；没有使用会连带隐藏子球的 `Visibility::Hidden`。右 grip 调试盒暂时保留。
- 右手：暖白 `SpotLight`，`120,000 lm`、`20 m` range、内角 `16°`、外角 `28°`、开启阴影，固定 `LightShadowMotionPolicy::FullyDynamic`，作为右 grip anchor 子实体。
- 射灯沿 Bevy local `-Z`，当前使用 grip pose；若真机握持方向不自然，应新增独立 OpenXR aim pose，禁止使用设备专属欧拉角补丁。
- 离开 Map_S03B、追踪丢失、anchor 变化或 XR session 结束时都会清理对应运行时实体。

### [实现] 通用 SpotLight 与动态 caster 阴影路径

- `shadow_motion_policy.rs` 支持 SpotLight policy。P1 尚无 spot cache，因此非 FullyDynamic 类会显式提升到真实单 frustum FullyDynamic，且不会错误挂载 PointLight cache/jitter 组件。
- 修复 dynamic caster 全局遮罩过宽的问题：缓存点光源继续使用 static + dynamic cubemap 双层；SpotLight 和 DirectionalLight 则在静态队列后重新加入动态 caster 的原生阴影 phase。
- vendor `bevy_pbr::queue_shadows` 先检查当前 `SHADOW_CASTER` 标志，再验证旧 phase cache，避免实体从 Static 迁移到 DynamicOverlay 后旧投影永久残留在静态层。
- 新增回归测试确认 Spot/Directional 会重入、Point static view 不会重入。

## 当前文件和修改状态

本阶段开始时工作区 clean，基线为上述 HEAD。以下功能与文档改动组成当前阶段提交：

```text
 M zevy_engine/src/app.rs
 M zevy_engine/src/scene.rs
 M zevy_engine/src/shadow_motion_policy.rs
 M zevy_engine/src/shadow_overlay.rs
 M zevy_engine/src/xr.rs
 M zevy_engine/third_party/crates/bevy_mod_openxr-0.3.0/src/openxr/action_binding.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/light.rs
?? zevy_engine/src/scene/map_s03b_xr_hand_test.rs
 M Docs/Checkpoints/CURRENT.md
?? Docs/Checkpoints/2026-07-24-map-s03b-xr-hand-shadow-harness.md
?? Docs/Checkpoints/2026-07-24-pico4s-runtime-binding-correction.md
?? Docs/Checkpoints/2026-07-24-xr-hand-shadow-harness-complete.md
 M zevy_engine/docs/VR_Renderring.md
```

另有用户独立调参 `M zevy_engine/src/config.rs`：`xr_render_scale 0.8 -> 1.0`，明确排除在本次功能提交之外并保留在工作区。`target/release/apk/zevy_engine.apk` 是 ignored 构建产物，不进入提交。

## 关键决定、产品不变量和禁止事项

- Map_S03B 只是 harness；追踪有效性、SpotLight policy 和 shadow phase 修复均为通用机制。
- `DynamicOverlay` 球和 `FullyDynamic` 射灯使用 fixed/manual policy，手停止时也不能自动降级。
- 灯光照射 `range` 与相机可见性继续分离；不得为解决可见性或阴影剔除问题扩大手电筒 range。
- 左右眼共享主世界手柄 pose、policy、灯和 caster 实体；禁止 per-eye 分类或随机状态。
- 失去追踪时移除对象，不得继续使用 stale pose。
- 射灯使用单 frustum 是结构性成本优势，但不能把理论成本写成 PICO 实测收益。
- 若 grip 朝向不符合手电筒语义，下一步使用 OpenXR `/input/aim/pose`；不得写只对当前 PICO 有效的旋转常量。
- 规范注册表只描述可用标准，不代表目标 runtime 已经支持、启用或激活对应 profile。interaction profile 必须由 API 版本、扩展枚举/启用与 `xrGetCurrentInteractionProfile` 的实机证据裁决。
- `pico4s_controller` 是当前目标运行时的产品事实，不得再次仅因 OpenXR 1.1 注册表命名而删除。Oculus/其他 profile 的 `XR_ERROR_PATH_UNSUPPORTED` 必须保持非致命，不能阻止后续已支持 profile 的建议绑定。
- 不引入第二套 XR 插件，不改 VR Camera/XR 起点，不改变导入器格式。

## 测试结果

### 已执行并通过

- `cargo fmt`
- `cargo test --lib`：63 passed，0 failed。
- 新增 3 项手柄夹具测试：正确父级与 fixed policy、tracking loss 清理、方向有效性与 Map scope。
- 新增 SpotLight policy 测试和动态 caster 非 Point shadow 重入测试。
- `cargo check --target aarch64-linux-android --message-format=short`。
- `cargo check --no-default-features --target aarch64-linux-android --message-format=short`：Shipping feature 组合通过。
- release + `render_debug` APK：编译、打包、4K alignment 和 debug keystore v3 签名验证通过。
- 修正后最终 APK：`G:\zevy_engine\zevy_engine\target\release\apk\zevy_engine.apk`，737,278,005 bytes，2026-07-24 11:17:44。
- APK SHA-256：`E4AD277D86C73407F36E366065F1BA2460D305086CED91BBB42BB13A14A59157`；APK v3 签名验证通过。
- ADB 设备 `PA9410MGJ9260457G` 在线；`adb install -r` 完成 incremental/streamed install，结果 `Success`。
- 冷启动成功，进程 PID `9217`，OpenXR session 进入 `FOCUSED`，Map_S03B 加载完成，无 panic、fatal shader error 或 Vulkan validation fatal。
- Runtime 日志确认设备为 PICO 4 Ultra Enterprise、Runtime 2.2.0，活动 profile 为 `/interaction_profiles/bytedance/pico4s_controller`。
- 左右 grip 都获得有效 tracking flags：日志确认生成左手 fixed `DynamicOverlay` 球和右手 fixed `FullyDynamic` shadowed SpotLight。
- 新增错误诊断确认当前运行时拒绝的是 Oculus Touch fallback：`Suggested path unsupported for interaction profile '/interaction_profiles/oculus/touch_controller'`；该错误非致命，后续 `pico4s_controller` 建议绑定和 action sync 正常工作。
- 左手调试盒隐藏后重新执行 `cargo fmt` 与 `cargo test --lib`：63 passed，0 failed。

唯一编译 warning 是既有 `bevy_mod_openxr` 的 elided lifetime 提示，与本阶段逻辑无关。

### 未执行

- 用户按键验收：右手 A/B、摇杆、扳机等动作必须实际按下确认；自动冷启动无法代替物理输入。
- PICO 佩戴视觉验证：左球位置/跟手、右灯握持方向、墙地面实时阴影、tracking loss 清理、左右眼一致。
- 最新“左 grip anchor 不渲染”源码尚未完成可确认的 APK 构建/安装；被用户中断的构建等待不能记为已通过。上一版已安装 APK 仍可能显示左调试盒。
- GPU P50/P95/P99、额外 SpotLight shadow pass 成本和 thermal soak。
- 第二独立场景或合成 XR hand fixture；Map_S03B 结果不能决定全局默认值。

## 未完成步骤、风险和唯一下一步

1. 从本阶段提交重新生成 APK 并安装，确认左手只显示 4 cm 球、不再显示 grip 调试盒。
2. 用户佩戴设备并验证 A/B、摇杆、扳机等按键已恢复，左球跟手、右手电筒存在且方向自然。
3. 验证左球在墙/地面投射连续动态阴影、tracking loss 无旧灯/旧影、左右眼一致。
4. 若只有光束方向不符合握持语义，建立独立 right aim Action Space 后重测；禁止靠 PICO 专属旋转常量修图。
5. 视觉正确后记录新增 SpotLight 的 GPU 成本，再决定是否需要 spot shadow cache、分辨率/更新预算或 flashlight priority。
6. 后续把 profile 选择演进为 API 版本/扩展能力协商；OpenXR 1.1 promoted PICO profile 只能在 runtime 明确支持后进入建议绑定集合。

唯一明确的下一步：**提交后从新 HEAD 生成并安装 APK，先确认左手调试盒消失且 4 cm DynamicOverlay 球仍可见/投影，再继续按键与手电筒 aim 验收。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Design\Shadow_Motion_Policies.md`
4. `G:\zevy_engine\zevy_engine\src\scene\map_s03b_xr_hand_test.rs`
5. `G:\zevy_engine\zevy_engine\src\xr.rs`
6. `G:\zevy_engine\zevy_engine\src\input.rs`
7. `G:\zevy_engine\zevy_engine\src\shadow_motion_policy.rs`
8. `G:\zevy_engine\zevy_engine\src\shadow_overlay.rs`
9. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_mod_openxr-0.3.0\src\openxr\action_binding.rs`
10. `G:\zevy_engine\zevy_engine\third_party\crates\bevy_pbr-0.16.1\src\render\light.rs`
11. 实际 `git status --short`、branch/HEAD、最新测试与 ADB 状态
