# 阶段检查点：UE Static PointLight 运行时语义

## 元数据

- 完成时间：2026-07-21 14:12，Asia/Shanghai
- 分支 / 起始 HEAD：`main @ 5cdde886647330e59678e00b548185ff33dbd3e2`
- 阶段状态：第一版自动化通过但被用户画面测试证伪；profile-once / animate-never 修正版已通过自动化与 Android 编译检查，等待用户画面复测
- 提交状态：未提交

## 目标与完成标准

让 UE 清单中的 `unreal.mobility = "static"` 在 Zevy 中具有真实静态语义：应用一次 Map_S03B authored-to-runtime 校准后固定有效参数，不播放蜡烛动画、不周期性使静态阴影失效，同时不破坏 movable 灯和持久化 shadow cache。

## 已完成

- UE 导出器原本已输出 mobility，Bevy 的 `ImportedZevyLight` 原本已保留完整定义，本阶段复用该权威数据。
- 新增大小写不敏感、可容忍空白的显式 static 判定；缺失字段不改变旧资源行为。
- Map_S03B profile 对 static PointLight：
  - 第一版错误地跳过强度 `×1000` 和范围 `×4` 的关卡校准；
  - 修正版与其他灯一样应用一次校准，之后固定校准结果；
  - 不生成 `MapS03BCandleGlow`；
  - 校准后不再更新 intensity/range/translation；
  - 不加入 candle shadow update candidates；
  - 保留 shadow residency 与首次生成后的静态缓存复用。
- 当前资产包含 18 个 PointLight：16 movable、2 static，因而已有真实回归对象。
- 规格、导出器限制说明和 VR shadow 文档已同步。

## 失败实验与修正

- [用户画面测试失败] 第一版把“static 不动画”解释成“不应用关卡 profile”，两盏 static 灯因此几乎没有照明效果。
- 根因不是 mobility 识别或 clustered culling，而是 static 灯停留在导出强度/范围；相对既有 Map 路径分别低 `1000×` 和 `4×`。
- 修正语义：所有 PointLight 先应用一次 Map 校准；static 固定校准后的结果，movable/stationary 才继续蜡烛动画。

## 文件状态

本阶段代码/文档修改：

- `zevy_engine/src/scene.rs`
- `zevy_engine/src/scene/zevy_level.rs`
- `Docs/UE_to_Bevy_Spec.md`
- `ue_project/Plugins/ZevyLevelExporter/README.md`
- `zevy_engine/docs/VR_Renderring.md`
- `Docs/Checkpoints/CURRENT.md`
- 本文件

独立的既有用户改动：

- `zevy_engine/src/config.rs`：shadow update budget `2 -> 8`，本阶段保留但未把它冒充为本阶段修改。

## 测试

- `cargo fmt --all`：通过。
- `cargo test --all-targets`：44 passed，0 failed。
- `cargo check --target aarch64-linux-android --message-format=short`：通过。
- `cargo check --no-default-features --all-targets --message-format=short`：通过。
- PowerShell 审计 Map 清单：18 PointLight = 16 movable + 2 static。
- 未运行 PC 画面、未构建/安装本阶段 Android APK、未进行 PICO 佩戴验证。

## 关键决定与禁止事项

- static 不只是“低频更新”，而是禁止 level-specific animation 修改一次性校准后的有效参数。
- static 灯仍会产生正常的 direct-light 与可选 shadow sampling 成本，不能把它记录成零成本。
- stationary 保持原行为，直到实现明确的 UE Stationary 对应策略。
- 不因本改动改变 exact-8、物理 range、相机可见策略或 shadow residency。

## 下一步

使用当前 Map_S03B 验证 `PointLight17` / `PointLight18` 静止、无蜡烛发光体动画，16 个 movable 灯仍正常闪烁；观察 static lights 在 warmup 后不再贡献周期性 shadow redraw。用户验证通过后再提交。
