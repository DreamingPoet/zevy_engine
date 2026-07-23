# 任务检查点：Map_S03B 自由飞行灯与动态投影球基准

## 元数据

- 更新时间：2026-07-22 15:36，Asia/Shanghai
- 状态：代码阶段已完成；PC/Android/VR 运行视觉验证待用户执行
- 工作区：`G:\zevy_engine`
- 分支与 HEAD：`main @ ed9f4647c9389f114176c2c9fa3fb2fa6bbe5817`
- 下一恢复入口：`Docs/Checkpoints/CURRENT.md`

## 最终目标和完成标准

- 最终目标：建立面向 VR 一体机的高性能、现代、大量动态灯光与动态阴影渲染器。
- 产品完成标准：灯和 caster 的不同运动类型能由通用策略选择缓存、代理、稀疏更新或完整动态路径，
  不依赖 Map 名称、固定坐标或手工实体名单；左右眼一致且没有距离 popping、漏光或旧影残留。
- 当前阶段完成标准：不修改 UE 导入内容，为 Map_S03B 独立添加 2 个飞行 PointLight 和 2 个飞行动态
  投影球；灯采用几何正确的真实动态阴影路径，球采用 dynamic caster overlay；轨迹可复现且代码通过
  Rust 与 Android 编译检查。

## 已完成内容

- [实现] 新增 `zevy_engine/src/scene/map_s03b_motion_test.rs`，只在当前 Level 精确为 Map_S03B 时生成测试体。
- [实现] 两盏灯分别为绿色和黄色，强度 `150,000 lm`、物理范围 `12 m`、半径 `0.08 m`、阴影开启；
  emissive 小球只用于显示光源位置，不投射或接收阴影。
- [实现] 飞行灯没有 `ImportedZevyLight`、`CachedPointLightShadow` 和 `PointLightShadowMapJitter`，不会被
  导入蜡烛 profile 接管，也不会错误地用静态 cubemap 代理米级位移。
- [实现] 两个半径 `0.38 m` 的 PBR 球显式添加 `DynamicShadowCaster`，由静态/动态双层阴影合成路径处理。
- [实现] 四个对象使用独立的确定性三维 Lissajous 轨迹；轨迹围绕玩家起点前方、两侧墙面布置，便于
  观察球在墙面和地面的动态投影。
- [实现] 只有 harness 根实体带 `LevelEntity`，运行时对象都通过 `ChildOf` 归属根；Level 切换时整体清理。
- [未运行验证] 没有启动 PC renderer，没有构建/安装新 APK，没有在 PICO 上观察画面。

## 当前文件和修改状态

本阶段新增/修改：

```text
 M zevy_engine/src/scene.rs
?? zevy_engine/src/scene/map_s03b_motion_test.rs
 M Docs/Checkpoints/CURRENT.md
?? Docs/Checkpoints/2026-07-22-map-s03b-flying-shadow-harness.md
```

工作区还保留连续 PointLight 阴影代理 P1 的全部未提交修改，包括根 `AGENTS.md`、配置、HUD、shader、
Zevy `bevy_pbr` fork、`VR_Renderring.md`、设计文档和上一阶段快照。它们不是本次重新实现的内容，不得
覆盖或拆除。完整清单以 `git status --short` 为准。

## 关键决定与禁止事项

- UE 导入数据继续视为静态场景；此次没有修改 Level JSON、glTF、UE 插件或导入层级。
- 自由飞行灯是 `FullyDynamic` 正确性/成本基线。米级位移必须真实重画 shadow cubemap，禁止使用仅为
  亚厘米 bounded motion 设计的虚拟投影原点。
- 动态球必须走 overlay，不能因其每帧变换而使整层静态缓存失效。
- 光照物理范围仍为 `12 m`，不得通过继续扩大 range 掩盖相机/视锥可见性问题。
- 两眼共享同一个 MainWorld Transform 和解析轨迹；不得引入 per-eye 时间、随机数或独立运动状态。
- 当前坐标、颜色、轨迹和实体数量是 Map 测试 harness，可以为观察效果调整，但不是通用 renderer 功能。
- 本阶段没有实现 `ShadowMotionPolicy`，不能把“手工选择完整动态路径”记录成自动分类已经完成。

## 测试结果

- [通过] `cargo fmt --all`。
- [通过] `cargo test map_s03b_motion_test --lib -- --nocapture`：3 passed，0 failed。
- [通过] `cargo test --all-targets`：49 passed，0 failed。
- [通过] `cargo check --target aarch64-linux-android --message-format=short`。
- [通过] `cargo check --no-default-features --all-targets --message-format=short`。
- [通过] `git diff --check`；只有既有 Windows LF→CRLF 提示。
- [未执行] PC 场景启动和截图。
- [未执行] Android APK 构建、签名、安装与启动。
- [未执行] PICO 佩戴视觉验证、HUD 计数、GPU capture 与 thermal soak。
- 已知非本阶段 warning：`bevy_mod_openxr` 的 lifetime syntax 提示。

## 未完成步骤和下一步

1. 用户运行 Map_S03B，确认绿色/黄色光点和墙面直接光可见，两个球均在移动。
2. 确认两球投影逐帧跟随、旧影能清除、没有漂浮/漏光，左右眼一致；HUD dynamic caster 应比旧构图多 2。
3. 若路径被遮挡或光照不易辨识，只调整 `map_s03b_motion_test.rs` 中的 harness 坐标、速度、亮度或材质，
   不修改导入资产与通用 renderer 规则。
4. harness 视觉有效后，把飞行灯作为 `FullyDynamic` reference，继续实现自动/Preferred/Forced
   `ShadowMotionPolicy` 与迟滞升级/降级。

唯一明确的下一步：**由用户在 PC 或 PICO 中运行 Map_S03B，反馈两盏灯、两个球及动态阴影的可见性和
连续性。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\zevy_engine\src\scene\map_s03b_motion_test.rs`
4. `G:\zevy_engine\zevy_engine\src\scene.rs`
5. `G:\zevy_engine\zevy_engine\src\shadow_cache.rs`
6. `G:\zevy_engine\zevy_engine\src\shadow_overlay.rs`
7. `G:\zevy_engine\Docs\Design\Continuous_Point_Shadow_Proxy.md`
8. 实际 `git status --short`、`git diff`、branch/HEAD
