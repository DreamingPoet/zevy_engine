# 当前任务检查点：Light / Shadow Caster Motion Policy P1

## 元数据

- 更新时间：2026-07-23 18:38，Asia/Shanghai
- 状态：P1 通用策略实现、单元测试、PC 场景验证、Android 编译与 APK 构建完成；用户已授权把当前累计渲染工作作为一次提交归档；ADB 安装失败，Android/VR 视觉验收未完成
- 工作区：`G:\zevy_engine`
- 分支 / 提交前基线 HEAD：`main @ ed9f4647c9389f114176c2c9fa3fb2fa6bbe5817`；本文件随本次提交归档，恢复时以实际 `git rev-parse HEAD` 为最终提交号
- 本阶段历史快照：`Docs/Checkpoints/2026-07-23-shadow-motion-policy-p1.md`
- 上一阶段：`Docs/Checkpoints/2026-07-23-map-s03b-light-patrol-orbits.md`
- 下一恢复入口：本文件

## 最终目标和完成标准

最终目标仍是建立面向 VR 一体机的、高性能、现代、支持大量动态灯光与动态阴影的 Zevy renderer。运动策略的目的，是把灯和 caster 按真实运动类型送入成本阶数不同的阴影路径，而不是靠隐藏远灯、扩大 `light.range`、降低灯数或按相机距离突然关闭阴影取得表面性能。

成本模型：若静态、微动、慢移和全动态灯数分别为 (L_s,L_m,L_k,L_f)，PointLight face 数 (F=6)，静态/动态 caster 数为 (S,D)，未分类路径近似为：

\[
C_{full}=O((L_s+L_m+L_k+L_f)F(S+D)).
\]

P1 分层目标为：

\[
C_{policy}\approx O(I_sL_sFS)+O(I_mL_mFS)+O(U_kL_kFS)+O(L_fF(S+D))+O((L_s+L_m+L_k)FD).
\]

自动分类 CPU 成本为 (O(L+B))，其中 (B) 为带策略的 caster root 数；策略只在类别变化时改变 ECS 路由组件。

本阶段完成标准：

1. 灯支持 Automatic 与手动 Static / BoundedMicroMotion / SlowMoving / FullyDynamic；自动路径立即升级、迟滞降级。
2. Caster 支持 Automatic 与手动 Static / DynamicOverlay；平移、旋转、缩放能触发迁移。
3. 策略在主世界统一计算，左右眼共享结果；不读取相机距离。
4. UE 导入实体、PerformanceLab、Map_S03B 蜡烛及飞行灯球都接入同一通用 API，Map 名不进入分类器。
5. 静态/动态迁移会清理旧影；旧 `DynamicShadowCaster` API 在没有 policy 时保持兼容。
6. Rust 全量测试、PC 场景、Android target 和 APK 构建通过；Android/VR 的无旧影、无漏光、双眼一致仍需真机动态观察。

## 已完成内容

### [设计]

- 新增 `Docs/Design/Shadow_Motion_Policies.md`，记录成本模型、状态机、P1 路由、迟滞、迁移正确性、kill criterion 与后续阶段。
- 明确 `SlowMoving` 的 P1 只是 cache-on-dirty。双快照 `KeyframedCrossFade`、稀疏 page 和 GPU-ms 调度尚未实现，不得写成已经解决慢移阴影阶梯。
- 自动模式不会把真实 Transform 的小位移静默伪装成旧 cubemap 的虚拟位移；BoundedMicroMotion 需要明确的 jitter motion signal。

### [实现] 通用策略与状态机

- 新增 `zevy_engine/src/shadow_motion_policy.rs`。
- 公开灯光 API：`LightShadowMotionPolicy`、`LightShadowMotionMode`、`LightShadowMotionClass`、`LightShadowAutomaticThresholds`、`ResolvedLightShadowMotion`。
- 公开 caster API：`ShadowCasterMotionPolicy`、`ShadowCasterMotionMode`、`ShadowCasterMotionClass`、`ShadowCasterAutomaticThresholds`、`ResolvedShadowCasterMotion`。
- 新增 `ShadowMotionPolicyTelemetry`，统计每帧灯 `S/M/K/F`、caster `S/D` 和迁移数。
- 自动灯测量世界线速度与 range-rate EMA；更动态类别立即升级，低成本类别必须稳定满足 `settle_seconds` 才降级。
- 导入灯 Automatic 初始为 Static，真实运动后立即升级；运行时未知灯初始为 FullyDynamic，稳定后再降级，优先保证正确性。
- 自动 caster 测量 root 世界平移、旋转和缩放 delta；运动立即进入 DynamicOverlay，稳定后回 Static。
- policy 模式在运行时改变时重置分类基线，避免从手动 BoundedMicroMotion 恢复 Automatic 后残留旧 jitter 状态。
- policy 删除时清理由 policy 接管的 cache、jitter、dynamic marker 与 resolved state；没有 policy 的旧 marker 继续工作。

### [实现] 阴影管线顺序与迁移

- `ShadowMotionPolicyPlugin` 已接入主 App。
- 策略系统在 `PostUpdate` 中运行：位于 `TransformSystem::TransformPropagate` 之后、`ShadowCacheSet::Finalize` 之前，并在末尾执行 deferred commands。
- `shadow_cache.rs` 新增明确的 `ShadowCacheSet::Finalize`，确保当前帧分类直接参与 cache/dynamic 集合构建。
- Static / BoundedMicroMotion / SlowMoving 路由到持久 cache；仅 BoundedMicroMotion 路由 jitter；FullyDynamic 保留真实每帧 shadow redraw。
- Static 与 DynamicOverlay 迁移通过现有静态 caster count invalidation 和 previous-active-face 清理保证旧层重建。root marker 继续通过父层级作用于后代 mesh。

### [实现] 通用接入与可观测性

- `lib.rs` 公开 re-export 所有 policy 数据类型。
- `scene/zevy_level.rs`：导入 Actor root 默认 `ShadowCasterMotionPolicy::automatic()`；UE `mobility=static` 的 PointLight 固定 Static，其余 PointLight 默认 Automatic。
- `scene/levels.rs`：PerformanceLab 的运动 cube 与有阴影的轨道灯改用 Automatic，不再手工指定底层 marker。
- `scene/map_s03b_motion_test.rs`：两颗飞行球改用 Automatic caster policy，两盏轨道灯改用 Automatic light policy。轨迹仍只是 Map_S03B 测试 harness。
- `scene.rs`：Map_S03B 16 盏蜡烛固定 BoundedMicroMotion，2 盏 UE Static 灯固定 Static；退出关卡时恢复进入前 policy。Map 脚本只表达测试运动，不实现通用分类。
- `render_debug.rs`：Overview 与 Lights 页面显示灯 `S/M/K/F`、caster `S/D` 和当前迁移数。
- `config.rs` 只同步了一个过期测试断言：运行默认 exact threshold 已是用户验证通过的 18，测试从旧值 8 改为 18；运行配置未改变。

### [PC 验证]

- Map_S03B 启动、Naga/wgpu shader 编译和 8 秒截图成功，无 panic/shader error。
- 截图：`zevy_engine/target/render_debug/Map_S03B_shadow_motion_policy_p1.png`。
- HUD 实测：`Motion L S/M/K/F = 2/16/0/2`，`C S/D = 43/2`；对应 18 个导入灯、2 个飞行灯、43 个静态 actor roots 与 2 个运动球。
- 画面保留原场景、蜡烛、黄/绿飞行灯与球，没有重新加入灰色测试地板。
- 单帧截图不能证明整段自动迁移、旧影清理、左右眼一致或性能收益。

### [Android 构建]

- `cargo check --target aarch64-linux-android` 通过。
- release + render-debug APK 编译、打包、4K 对齐和签名通过。
- APK：`zevy_engine/target/release/apk/zevy_engine.apk`，737,253,429 bytes，2026-07-23 17:54:09。
- `PA9410MGJ9260457G` 在安装前为 online/device。
- ADB incremental install 失败：读取 APK 返回 `Bad file descriptor`。已按用户要求立即通知；包未安装，未冷启动，不能标记为 Android/VR 已验证。

## 当前文件和修改状态

本次用户授权的单次提交范围包含整条累计渲染开发链：continuous proxy、PBR fork、scalable lighting、dynamic overlay GPU preprocessing、Map harness、Motion Policy P1、AGENTS 与对应 checkpoint。提交后预期工作区 clean；若恢复时有新改动，以实际 Git 状态为准并保护它们。

提交前 `git status --short` 清单：

```text
 M AGENTS.md
 M Docs/Checkpoints/CURRENT.md
 M zevy_engine/docs/VR_Renderring.md
 M zevy_engine/src/app.rs
 M zevy_engine/src/config.rs
 M zevy_engine/src/lib.rs
 M zevy_engine/src/render_debug.rs
 M zevy_engine/src/scalable_lighting.rs
 M zevy_engine/src/scene.rs
 M zevy_engine/src/scene/levels.rs
 M zevy_engine/src/scene/zevy_level.rs
 M zevy_engine/src/shaders/zevy_pbr_functions.wgsl
 M zevy_engine/src/shadow_cache.rs
 M zevy_engine/src/shadow_overlay.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/lib.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/light/mod.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/light/point_light.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/light.rs
 M zevy_engine/third_party/crates/bevy_pbr-0.16.1/src/render/mesh_view_types.wgsl
?? Docs/Checkpoints/2026-07-22-continuous-point-shadow-proxy.md
?? Docs/Checkpoints/2026-07-22-map-s03b-flying-shadow-harness.md
?? Docs/Checkpoints/2026-07-23-dynamic-caster-overlay-gpu-preprocess.md
?? Docs/Checkpoints/2026-07-23-map-s03b-light-patrol-orbits.md
?? Docs/Checkpoints/2026-07-23-shadow-motion-policy-p1.md
?? Docs/Design/
?? zevy_engine/src/scene/map_s03b_motion_test.rs
?? zevy_engine/src/shadow_motion_policy.rs
```

本阶段主要代码文件：

- `zevy_engine/src/shadow_motion_policy.rs`：新通用策略实现。
- `zevy_engine/src/app.rs`、`lib.rs`、`shadow_cache.rs`：插件接入、公共 API 和执行顺序。
- `zevy_engine/src/scene/zevy_level.rs`、`scene/levels.rs`、`scene.rs`、`scene/map_s03b_motion_test.rs`：导入器、通用测试场景和 Map harness 接入。
- `zevy_engine/src/render_debug.rs`：策略遥测。
- `Docs/Design/Shadow_Motion_Policies.md`、`zevy_engine/docs/VR_Renderring.md`：设计与工程状态。

ignored 产物：PC 截图与 APK 位于 `target`，不进入提交。

## 关键决定、产品不变量和禁止事项

- Map_S03B 只是测试 harness。通用策略模块不检查 Map、Actor 名、固定坐标或固定灯数。
- 灯的物理 `range` 与相机可见性继续分离；policy 不按相机远近开关灯或阴影。
- 左右眼共享主世界分类、历史、jitter、cache 和 dynamic marker；禁止 per-eye 自动判断。
- 手动模式固定优化路径；Automatic 可以在运行时升降级。运动升级优先于性能，降级必须迟滞。
- Static 灯不添加蜡烛动画；BoundedMicroMotion 只适用于量化/误差边界内的明确虚拟运动。
- 自由飞行灯不能使用旧 cubemap 的微动代理掩盖 disocclusion；本阶段轨道灯保持 FullyDynamic reference。
- 动态 caster 不得使整座静态场景每帧重画；继续使用 static cache + dynamic overlay。
- `SlowMoving` P1 仍会在连续 Transform 改变时重画。不能把它宣传为无阶梯低频阴影。
- 若自动 caster 回静态时不能可靠清除旧影，应保持 DynamicOverlay，不能为了省成本提前降级。
- 不覆盖、不丢弃用户/前序工作。本次只执行用户明确授权的一次本地提交，不推送远端。

## 测试结果

### 已执行并通过

- `cargo fmt --check`
- `cargo test shadow_motion_policy --lib`：6 passed
- `cargo test map_s03b_motion_test --lib`：5 passed
- `cargo test --lib`：58 passed，0 failed
- `cargo check --target aarch64-linux-android`
- PC：`cargo run -- --level=levels/Map_S03B/Map_S03B.zevy-level.json --screenshot=...Map_S03B_shadow_motion_policy_p1.png --screenshot-delay=8`
- Android release/render-debug APK build、4K alignment、签名

仅有的编译 warning 是前序 `bevy_mod_openxr` 的 elided lifetime 提示，与本阶段无关。

### 已执行但失败

- `adb -s PA9410MGJ9260457G install --incremental -r ...zevy_engine.apk`
- 结果：`Failed to stat input file ...: Bad file descriptor`。设备查询本身显示 `device`，但 APK 未传输成功。

### 未执行

- Android 安装后的冷启动与 logcat panic/shader 检查。
- PICO 佩戴验证：自动灯/球运动时的阴影连续性、停止后降级、再次运动升级、旧影清理、漏光、漂浮和左右眼一致。
- 第二独立场景覆盖 Static / Micro / Slow / FullyDynamic。
- 16→32→64 灯增长斜率、AGI capture、GPU P50/P95/P99、20～30 分钟 thermal soak。

## 未完成步骤、风险和唯一下一步

1. 用户恢复 ADB 安装能力后，优先安装现有 APK；无需重新编译。若 incremental 仍失败，使用正常 full install 路径，但设备问题由用户处理。
2. 冷启动 Map_S03B，确认日志无 panic/shader error，HUD 分类应稳定接近 `2/16/0/2` 与 `43/2`。
3. 佩戴观察运动球与飞行灯：阴影跟随、离开后无旧影、停止/重启运动时无突然漏影、左右眼一致。当前 harness 持续运动，若要实测降级状态，需要增加通用 pause/resume 合成测试，而不是写 Map 坐标特例。
4. P1 真机视觉通过后，下一代码阶段实现 `SlowMoving` 双快照/时间重建原型，并加入 priority、stale time、GPU-ms token budget；并行增加 UE manifest 的 per-light/per-actor policy authoring schema。
5. 第二独立合成场景必须验证不同灯数、速度、尺度和遮挡，Map_S03B 单场景不能决定默认阈值。

唯一明确的下一步：**等待用户恢复 APK 安装，然后安装已经生成的 release APK，完成 Motion Policy P1 的 Android/VR 动态正确性验收。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Design\Shadow_Motion_Policies.md`
4. `G:\zevy_engine\zevy_engine\src\shadow_motion_policy.rs`
5. `G:\zevy_engine\zevy_engine\src\shadow_cache.rs`
6. `G:\zevy_engine\zevy_engine\src\shadow_overlay.rs`
7. `G:\zevy_engine\zevy_engine\src\scene\zevy_level.rs`
8. `G:\zevy_engine\zevy_engine\src\scene\map_s03b_motion_test.rs`
9. 实际 `git status --short`、branch/HEAD、最新测试、ADB 状态
