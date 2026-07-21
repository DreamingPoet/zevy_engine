# 当前任务检查点：UE Static 灯光保持静态

## 元数据

- 更新时间：2026-07-21 14:19，Asia/Shanghai
- 状态：第一版已被用户画面测试证伪；profile-once / animate-never 修正版已通过自动化和 Android 编译检查，等待用户画面复测
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：`main @ 5cdde886647330e59678e00b548185ff33dbd3e2`
- 本阶段历史快照：`Docs/Checkpoints/2026-07-21-static-light-mobility.md`
- 上一视觉基线：`Docs/Checkpoints/2026-07-21-world-stable-lighting-exact8.md`

## 最终目标和完成标准

最终目标仍是建立面向 VR 一体机的高性能、现代、大量动态灯光/阴影渲染器。当前任务的完成标准是：

1. UE 导出的 `lights[].unreal.mobility = "static"` 成为运行时权威静态标记；
2. Map_S03B 对 static PointLight 应用一次关卡亮度/范围校准，随后不得再修改颜色、有效亮度、有效范围和 Transform；
3. static PointLight 不生成/播放蜡烛发光体动画，不进入 candle shadow 周期性失效队列；
4. static 灯若启用阴影，仍正常常驻并在首次生成后复用持久化 shadow cache；
5. movable 蜡烛灯现有动画、exact-8 画质基线和相机无关 shadow residency 不回归。

## 已完成内容

### [已实现]

- `ZevyUnrealLightParameters::is_static_mobility()`：trim 后大小写不敏感匹配显式 `static`。
- 旧清单缺失 mobility、`movable`、`stationary` 均不会被误判为 static。
- `apply_map_s03b_lighting_profile` 直接读取实体上的 `ImportedZevyLight`：
  - movable/stationary 保持现有 Map 蜡烛倍率与动画路径；
  - static 与其他灯一样先应用一次 Map authored-to-runtime 强度/范围校准；
  - 校准后的 static intensity、range 与 Transform 不再随时间变化；
  - static 仍获得 `CachedPointLightShadow`，可以使用持久化静态阴影缓存。
- `sync_map_s03b_candle_visuals` 不为 static 灯生成发光球；若状态已存在也会移除。
- `animate_map_s03b_candle_lights` 在任何亮度、范围、Transform 或 shadow invalidation 写入前跳过 static 灯。
- 当前 Map_S03B 资产审计：18 个 PointLight，其中 16 个 `movable`、2 个 `static`。
- 更新 `Docs/UE_to_Bevy_Spec.md`、导出器 README 与 `zevy_engine/docs/VR_Renderring.md`。

### [自动化验证]

- 新测试验证只有显式 `static` 才进入静态语义。
- 新 ECS 系统链测试验证 static PointLight：
  - intensity/range 只应用一次 Map profile，跨后续时间步保持固定；
  - Transform 保持导出值；
  - shadow residency 保持启用；
  - 不写入 shadow refresh 时间；
  - 不生成 `MapS03BCandleVisualSpawned`。

## 当前文件与 Git 状态

阶段起点为已提交的 `5cdde88`。开始本任务前已有一项用户改动，必须保留且不得混淆为本阶段实现：

- `zevy_engine/src/config.rs`：用户把 `max_cached_point_shadow_updates_per_frame` 从 `2` 改为 `8`。

本阶段修改：

- `zevy_engine/src/scene.rs`
- `zevy_engine/src/scene/zevy_level.rs`
- `Docs/UE_to_Bevy_Spec.md`
- `ue_project/Plugins/ZevyLevelExporter/README.md`
- `zevy_engine/docs/VR_Renderring.md`
- `Docs/Checkpoints/CURRENT.md`
- `Docs/Checkpoints/2026-07-21-static-light-mobility.md`

所有修改均未暂存、未提交。禁止 reset/checkout/覆盖用户的 shadow budget 修改。

## 关键决定、产品不变量与禁止事项

- mobility 已由 UE 导出并完整保存在 `ImportedZevyLight`，不新增重复资产字段。
- `static` 控制的是时间变化，不得跳过 Map_S03B 必需的一次性 authored-to-runtime 校准。
- static 只消除 candle 动画与周期性 shadow redraw；它仍有直接光照、cluster、shadow residency 和采样成本。
- `stationary` 的 UE 混合光照语义尚未完整实现，本阶段不擅自将其等同 static。
- 不改变灯光物理 range、相机可见距离、exact-8 选择路径或阴影是否由相机距离决定。
- 不得把自动化测试写成 Android/VR 视觉验证。

## 实际测试结果

### 已执行并通过

- `cargo fmt --all`
- `cargo test --all-targets`：44 passed，0 failed
- `cargo check --target aarch64-linux-android --message-format=short`
- `cargo check --no-default-features --all-targets --message-format=short`
- Map_S03B JSON 审计：18 PointLight = 16 movable + 2 static

### 非阻塞警告

- vendored `bevy_mod_openxr` 仍有既有 `mismatched_lifetime_syntaxes` warning，本阶段未引入。

### 用户证伪并已修正

- 失败版本让 static 灯完全跳过 Map profile，导致运行时强度比既有路径低 1000 倍、范围小 4 倍；用户观察到 static 灯对场景没有可见效果。
- 修正策略：所有灯 profile-once；只有非 static 灯 animate-many。修正后的 Android/VR 画面尚待用户复测。

### 尚未执行

- PC Map_S03B 实际运行与画面检查
- Android APK 构建/安装
- PICO 佩戴验证 static 灯完全稳定、movable 蜡烛继续动画
- static 灯加入后 fixed-path GPU P50/P95/P99 与 shadow redraw telemetry

## 未完成步骤、风险和唯一下一步

1. 在 Map_S03B 实际运行中确认 `PointLight17`、`PointLight18` 产生正常照明，并保持校准后亮度/范围与导出位置不变，且没有发光球动画。
2. 确认 16 个 movable 灯继续闪烁，static 灯不会增加周期性 `updated faces`。
3. 用户确认画面后，再决定是否把用户的 shadow budget=8 与本阶段代码合并提交。

唯一明确的下一步：**由用户在 PC/VR 运行当前 Map_S03B，验证两盏 static 灯完全不动且 movable 蜡烛无回归；若通过，再提交本阶段。**

## 恢复时首先读取

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\Docs\Checkpoints\2026-07-21-static-light-mobility.md`
4. `G:\zevy_engine\zevy_engine\src\scene.rs`
5. `G:\zevy_engine\zevy_engine\src\scene\zevy_level.rs`
6. `G:\zevy_engine\Docs\UE_to_Bevy_Spec.md`
7. 实际 `git status --short`、`git diff`、branch/HEAD
