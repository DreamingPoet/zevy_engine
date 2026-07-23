# 任务检查点：Light / Shadow Caster Motion Policy P1

## 元数据

- 更新时间：2026-07-23 17:58，Asia/Shanghai
- 状态：P1 实现、PC 验证、Android 编译/APK 完成；ADB 安装失败，VR 验收待完成；未暂存、未提交
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：`main @ ed9f4647c9389f114176c2c9fa3fb2fa6bbe5817`
- 恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和完成标准

最终目标是面向 VR 一体机的、高性能、现代、大量动态灯光/动态阴影 renderer。P1 要把运动语义从 Map 脚本下沉为通用引擎状态机：灯支持 Automatic 与手动 Static / BoundedMicroMotion / SlowMoving / FullyDynamic；caster 支持 Automatic 与手动 Static / DynamicOverlay；自动路径运动时立即升级、稳定后迟滞降级；结果由双眼共享且不依赖相机距离。

若灯数按运动类型为 (L_s,L_m,L_k,L_f)，PointLight face 数 (F=6)，静态/动态 caster 为 (S,D)，目标是从：

\[
O((L_s+L_m+L_k+L_f)F(S+D))
\]

分解为静态失效、微动失效、慢移更新、全动态重画和动态 caster overlay 五类成本。P1 分类 CPU 为 (O(L+B))。

## 已完成内容

### [设计]

- 新增 `Docs/Design/Shadow_Motion_Policies.md`，记录数学模型、状态语义、路由、迟滞、迁移清理、kill criterion 与后续计划。
- 明确 P1 `SlowMoving` 只是 cache-on-dirty；双快照/重建、稀疏 page、priority/stale/GPU-ms 调度尚未实现。

### [实现]

- 新增 `zevy_engine/src/shadow_motion_policy.rs`，公开两组 policy/mode/class/threshold/resolved API 和 `ShadowMotionPolicyTelemetry`。
- 灯自动分类使用世界线速度、range-rate EMA；动态等级立即升级，低成本等级需要 `settle_seconds`；导入灯初始 Static，运行时未知灯初始 FullyDynamic。
- 灯路由：Static=cache；BoundedMicroMotion=cache+jitter；SlowMoving=cache-on-dirty；FullyDynamic=真实 moving-light redraw。
- Caster 自动分类测量世界平移、旋转、缩放；运动进入 DynamicOverlay，稳定后回 Static。Actor root marker 通过父层级覆盖后代 mesh。
- 运行时 mode 热切换会重置状态基线；删除 policy 会清理接管的底层组件。没有 policy 的旧 `DynamicShadowCaster` API 兼容。
- 新增 `ShadowCacheSet::Finalize`；policy 固定在 `TransformPropagate` 后、shadow cache 最终分类前运行，并应用 deferred commands。
- `app.rs` 接入插件，`lib.rs` 公开 API，`render_debug.rs` 显示灯 `S/M/K/F` 与 caster `S/D`。
- 导入 Actor root 默认 Automatic caster；UE Static PointLight 固定 Static，其余 PointLight 默认 Automatic。
- PerformanceLab 的运动 cube/灯、Map_S03B 两颗飞行球/两盏飞行灯改用 Automatic。
- Map_S03B 16 盏蜡烛固定 BoundedMicroMotion、2 盏 UE Static 灯固定 Static；离开 Map 时恢复原 policy。Map 代码只产生测试运动，不实现通用分类。
- 同步 `config.rs` 的过期测试期望 8→18，以匹配已由用户验证通过的实际 exact-light 默认值；运行配置未改变。

### [PC 验证]

- `cargo test --lib` 58/58。
- Map_S03B 启动、shader 编译、8 秒截图通过，无 panic/shader error。
- 截图 `zevy_engine/target/render_debug/Map_S03B_shadow_motion_policy_p1.png` 中 HUD 为：灯 `2 Static / 16 Micro / 0 Slow / 2 FullyDynamic`，caster `43 Static / 2 DynamicOverlay`。
- 场景保留两颗运动球与黄/绿飞行灯，没有灰色校准地板。静态截图不能证明整段迁移、旧影清理、双眼一致或性能收益。

### [Android 构建]

- Android target check 通过。
- release/render-debug APK 编译、打包、4K 对齐、签名通过。
- APK：`zevy_engine/target/release/apk/zevy_engine.apk`，737,253,429 bytes，2026-07-23 17:54:09。
- 设备 `PA9410MGJ9260457G` 查询为 online/device，但 incremental install 读取 APK 时返回 `Bad file descriptor`；已立即通知用户。未安装、未启动、未进行 VR 验证。

## 当前文件和修改状态

分支仍为 `main`，HEAD 未变化；没有 staged/commit。当前 dirty worktree 同时包含用户/前序 continuous proxy、PBR fork、scalable lighting、dynamic overlay GPU preprocessing、Map harness、AGENTS 与历史 checkpoint，必须整体保护。

本阶段新增或主要修改：

- `zevy_engine/src/shadow_motion_policy.rs`
- `zevy_engine/src/app.rs`
- `zevy_engine/src/lib.rs`
- `zevy_engine/src/shadow_cache.rs`
- `zevy_engine/src/render_debug.rs`
- `zevy_engine/src/scene/zevy_level.rs`
- `zevy_engine/src/scene/levels.rs`
- `zevy_engine/src/scene.rs`
- `zevy_engine/src/scene/map_s03b_motion_test.rs`
- `zevy_engine/src/config.rs`（仅同步旧测试断言）
- `Docs/Design/Shadow_Motion_Policies.md`
- `zevy_engine/docs/VR_Renderring.md`
- `Docs/Checkpoints/CURRENT.md`
- 本文件

当前完整状态以同时间的 `Docs/Checkpoints/CURRENT.md` 和实际 `git status --short` 为准。PC 截图与 APK 位于 ignored `target`，不进入提交。

## 关键决定与禁止事项

- Map_S03B 仅是 harness；分类器不得读取 Map/Actor 名、坐标或固定灯数。
- 不扩大 `light.range`、不按相机距离开关灯/阴影、不隐藏远灯。
- 左右眼共享分类与历史；禁止 per-eye policy。
- Automatic correctness-first：升级立即、降级迟滞；手动模式固定路径。
- Static 灯不做蜡烛动画；真实自由飞行灯不能使用微动旧 cubemap 冒充正确阴影。
- Dynamic caster 继续走 static cache + overlay，不能使整座静态场景每帧失效。
- 若回静态不能清理旧影，应保持 DynamicOverlay。
- P1 不宣称解决 SlowMoving 阶梯，也不宣称已有真机性能收益。
- 不覆盖当前未提交的用户/前序改动；未经请求不提交。

## 测试结果

已通过：

- `cargo fmt --check`
- `cargo test shadow_motion_policy --lib`：6 passed
- `cargo test map_s03b_motion_test --lib`：5 passed
- `cargo test --lib`：58 passed，0 failed
- `cargo check --target aarch64-linux-android`
- PC Map_S03B 启动、截图、无 shader/panic error
- Android release/render-debug APK build、4K alignment、签名

已失败：

- ADB incremental install：`Bad file descriptor`。失败发生在 APK 读取/传输前，不能记为 Android 已安装。

未执行：

- Android 冷启动/logcat。
- VR 动态迁移、旧影/漏光/漂浮、左右眼一致。
- 第二独立场景、16→32→64 灯斜率、AGI、GPU P50/P95/P99、thermal soak。

仅有编译 warning 是前序 `bevy_mod_openxr` elided lifetime 提示。

## 未完成步骤和下一步

1. 用户恢复 ADB 安装能力后直接安装现有 APK，无需重编译。
2. 冷启动 Map_S03B，确认无 panic/shader error，HUD 分类与 PC 基线一致。
3. 佩戴验证两颗球与两盏飞行灯的阴影跟随、旧影清理、无漏光/漂浮和左右眼一致。
4. 如需验证 Automatic 降级/再次升级，增加通用 pause/resume 合成 fixture，不能写 Map 坐标补丁。
5. 真机通过后进入 SlowMoving 双快照/时间重建、priority/stale/GPU-ms 调度、稀疏 page，并增加 UE per-light/per-actor authoring schema。

唯一下一步：**恢复 APK 安装并完成 Motion Policy P1 的 Android/VR 动态正确性验收。**

## 恢复时首先读取

1. `AGENTS.md`
2. `Docs/Checkpoints/CURRENT.md`
3. `Docs/Design/Shadow_Motion_Policies.md`
4. `zevy_engine/src/shadow_motion_policy.rs`
5. `zevy_engine/src/shadow_cache.rs`
6. `zevy_engine/src/shadow_overlay.rs`
7. `zevy_engine/src/scene/zevy_level.rs`
8. `zevy_engine/src/scene/map_s03b_motion_test.rs`
9. 实际 Git、测试和 ADB 状态
