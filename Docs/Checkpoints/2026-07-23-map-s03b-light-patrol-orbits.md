# 任务检查点：Map_S03B 场景灯位巡游球与轨道动态灯

## 元数据

- 更新时间：2026-07-23 17:20，Asia/Shanghai
- 状态：代码、单元测试、PC 场景视觉和 Android APK 构建完成；设备离线，安装/VR 验证待完成；未提交
- 工作区：`G:\zevy_engine`
- 分支 / HEAD：`main @ ed9f4647c9389f114176c2c9fa3fb2fa6bbe5817`

## 目标和完成标准

在不改变 UE 导出内容和通用阴影管线的前提下，删除 Map_S03B 灰色 calibration geometry，让两个动态投影球在导入灯位附近巡游，并让黄/绿 fully dynamic PointLight 分别跟随并围绕对应球公转。PC 用于正确性检查，最终由 PICO 动态视觉裁决。

## 已实现

- 删除灰色 floor/wall calibration receiver 及其 mesh、material、component 和固定投影配对逻辑。
- 从 `ImportedZevyLight` 查询非 Static PointLight 的 `GlobalTransform`；Map_S03B 当前得到 16 个 movable 灯位。
- 按两侧灯带分组、按 `x` 排序，向室内偏移 1.6 m并低于灯位 1.15 m，构造正向/反向闭合巡游路径。
- segment 内使用 smoothstep，保持点在线段内，避免 cubic overshoot 穿墙；附加小幅确定性垂直运动。
- 两个球继续带 `DynamicShadowCaster`；绿色/黄色 PointLight 作为各自球的子实体，以相反方向和不同相位公转。
- 测试 PointLight 保持 150,000 lm、12 m、真实 transform 和 shadow redraw；没有 cache/jitter 近似。
- 运行时没有复制具体灯坐标；单元测试中的灯位只是 fixture。

## 验证结果

- `cargo test map_s03b_motion_test --lib`：5 passed。
- `cargo fmt --all -- --check`：通过。
- `cargo check --target aarch64-linux-android`：通过。
- 全部 lib 测试：51 passed / 1 failed；唯一失败仍是前序 exact threshold 实际 18、旧测试断言 8。
- PC Map_S03B 日志确认 16 个 imported movable-light anchors、2 个 balls、2 个 orbiting PointLights、no calibration geometry。
- PC 截图：`zevy_engine/target/render_debug/Map_S03B_scene_light_patrol_orbits.png`；灰色测试地板/墙消失，球和黄/绿光点位于场景两侧灯带附近。
- Android release/render-debug APK 构建、zipalign、签名成功：`zevy_engine/target/release/apk/zevy_engine.apk`，737,220,661 bytes，2026-07-23 17:18:59。
- 安装未执行成功：设备 `PA9410MGJ9260457G` not found，ADB 列表为空；未做本阶段 VR 视觉验证。

## 文件与工作区

- 本阶段核心：`zevy_engine/src/scene/map_s03b_motion_test.rs`。
- 状态：`Docs/Checkpoints/CURRENT.md` 与本快照。
- 其余 AGENTS、continuous proxy、PBR fork、shader、shadow overlay、config/HUD 等 dirty 文件属于用户或前序未提交工作，必须保留。
- 无暂存、无提交；截图/APK 为 ignored build artifacts。

## 关键约束

- Map_S03B 是 harness，不是 renderer 架构前提。
- 不扩大 `light.range`，不提高强度掩盖错误，不关闭远灯/阴影。
- 球走 dynamic caster overlay；轨道灯的米级运动走真实 cubemap redraw。
- 时间/轨迹不依赖眼睛，左右眼必须共享状态。
- 场景重导后路径应由新导入灯位自动生成；不得为单个灯或 Actor 写产品路径坐标补丁。

## 下一步

设备恢复后安装已有 APK，验证球沿灯带巡游、黄/绿灯绕球公转、动态阴影及旧影清除、左右眼一致；视觉通过后继续通用 motion policy 和双眼 GPU preprocessing 去重。

## 恢复入口

1. `G:\zevy_engine\AGENTS.md`
2. `G:\zevy_engine\Docs\Checkpoints\CURRENT.md`
3. `G:\zevy_engine\zevy_engine\src\scene\map_s03b_motion_test.rs`
4. `G:\zevy_engine\zevy_engine\src\shadow_overlay.rs`
5. 实际 Git/测试/ADB 状态
