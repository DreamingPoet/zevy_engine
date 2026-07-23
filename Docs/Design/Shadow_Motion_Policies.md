# Zevy Light / Shadow Caster Motion Policies

## 目标

`LightShadowMotionPolicy` 与 `ShadowCasterMotionPolicy` 把灯光/遮挡物的运动语义从 Map 脚本下沉为通用引擎数据和状态机。目标不是用一个参数覆盖所有运动，而是按真实运动类型选择成本阶数不同的阴影路径，并允许自动判断或人工固定。

Map_S03B、PerformanceLab 和未来合成压力场景只是输入。产品 renderer 不得检查 Map 名、Actor 名、固定坐标或固定灯数。

## 成本模型

设有 $L_s$ 个静态灯、$L_m$ 个微运动灯、$L_k$ 个慢移/关键帧灯、$L_f$ 个全动态灯；每盏 PointLight 有 $F=6$ 个 shadow faces；静态 caster 数为 $S$，动态 caster 数为 $D$。

不分类的完整动态阴影成本近似为：

$$
C_{full}=O((L_s+L_m+L_k+L_f)F(S+D)).
$$

分层后的目标为：

$$
C_{policy}\approx O(I_sL_sFS)+O(I_mL_mFS)+O(U_kL_kFS)+O(L_fF(S+D))+O((L_s+L_m+L_k)FD),
$$

其中：

- $I_s$：静态内容失效事件，稳定时趋近 0；
- $I_m$：微运动代理超界或静态内容变化事件，稳定时趋近 0；
- $U_k$：慢移/关键帧真实更新率，P1 由真实 Transform 改变触发；
- 最后一项是 dynamic caster overlay，只绘制 $D$，不使静态层整层失效。

P1 的自动分类每帧每实体只做常数次向量/标量运算，CPU 成本为 $O(L+B)$，其中 $B$ 是带策略的 caster root 数。策略切换只在状态改变时增删 ECS 组件。

## 灯光策略

### 控制模式

- `Automatic`：引擎测量世界位移速度、range 变化率和虚拟原点 offset，并用迟滞选择 resolved class。
- `Static`、`BoundedMicroMotion`、`SlowMoving`、`FullyDynamic`：人工固定 resolved class；不会因观测结果自动改变。

### Resolved class 与 P1 路由

| Class | 持久 shadow cache | `PointLightShadowMapJitter` | P1 行为 |
| --- | --- | --- | --- |
| `Static` | 是 | 否 | 静态深度只在几何/投影失效时重画 |
| `BoundedMicroMotion` | 是 | 是 | 真实投影保持稳定，虚拟原点连续偏移 |
| `SlowMoving` | 是 | 否 | Transform/range 改变时真实重画，静止帧复用 |
| `FullyDynamic` | 否 | 否 | 保留 Bevy 原生真实 moving-light redraw |

`SlowMoving` 的双快照 `KeyframedCrossFade`、稀疏 pages 和 GPU-ms 调度尚未在 P1 实现。P1 不能把“cache-on-dirty”写成已经完成低频无阶梯重建。

### 自动分类

每灯保存前一帧世界位置 $p_{t-1}$、range $r_{t-1}$ 和平滑速度：

$$
v_t=\alpha\frac{\lVert p_t-p_{t-1}\rVert}{\Delta t}+(1-\alpha)v_{t-1},
$$

$$
q_t=\alpha\frac{|r_t-r_{t-1}|}{\Delta t}+(1-\alpha)q_{t-1}.
$$

自动 raw class：

1. 真实投影稳定且 jitter offset 非零、未超过可表示/允许边界：`BoundedMicroMotion`；
2. 真实投影速度低于静态阈值且无 jitter：`Static`；
3. 低于慢移速度/range-rate 阈值：`SlowMoving`；
4. 否则：`FullyDynamic`。

更高动态等级立即升级；向更低成本等级降级必须让同一候选持续 `settle_seconds`。随机运动灯停止后可以自动降级，重新运动时下一帧立即升级。

自动模式不会把真实 Transform 的小幅移动偷偷解释成虚拟原点；`BoundedMicroMotion` 只在调用者明确提供 `PointLightShadowMapJitter` motion signal 时成立。这样避免用旧 cubemap 冒充具有 disocclusion 的真实移动。

## Caster 策略

### 控制模式

- `Automatic`：测量 root 的世界平移、旋转和缩放变化；运动立即进入 `DynamicOverlay`，稳定达到迟滞时间后回到 `Static`。
- `Static`：固定进入静态 shadow layer。
- `DynamicOverlay`：固定进入动态 overlay。

策略可以放在 mesh 或 Actor root。现有层级遍历会让 root 的 resolved marker 作用于后代 mesh。

### 迁移正确性

- `Static -> DynamicOverlay`：从静态 caster 集合移除，并加入动态层；静态 caster 数变化触发静态 cache 失效，清除旧静态影。
- `DynamicOverlay -> Static`：从动态层移除并重新烘入静态层；动态 overlay 的 previous-active-face 清除旧动态影，静态 cache 同时失效重画。

策略只管理由自身创建/接管的 `DynamicShadowCaster` marker。没有策略组件的旧 API 保持兼容。

## 初始状态与双眼

- 导入 Actor/灯初始按静态场景假设进入低成本状态；一旦世界 Transform 真正变化，自动模式立即升级。
- 运行时新建且无法证明静态的灯/caster 初始走 correctness-first 的动态状态，稳定后再降级。
- 观测和状态机只在主世界运行一次，使用全局时间与世界 Transform；不读取 eye、screen tile 或相机距离。左右眼共享 resolved class、cache、marker 和历史。

## P1 验收与失败条件

必须验证：

1. 手动四类灯能得到正确 cache/jitter 路由；
2. 自动灯运动时立即升级，停止后迟滞降级，随机再次运动可重新升级；
3. 自动 caster 运动时进入 overlay，停止后重新进入静态层且无旧影；
4. UE Static 灯不会动画；Map_S03B 微运动蜡烛仍走代理；飞行灯/球不再依赖手工 cache/marker；
5. PC、Android 编译通过，VR 中无左右眼差异、旧影、漏光或相机距离 popping。

Kill criterion：若自动迁移在同一帧无法保证静态旧影清除，则保持动态状态而不是提前降级；若 `BoundedMicroMotion` 超出误差/量化边界，必须升级真实更新路径，禁止 clamp 后冒充正确。

## P1 实现状态（2026-07-23）

- [实现] `zevy_engine/src/shadow_motion_policy.rs` 提供公开 policy、mode、class、自动阈值、resolved 结果和汇总 telemetry；系统固定在主世界 `PostUpdate`、`TransformPropagate` 之后、`ShadowCacheSet::Finalize` 之前运行。
- [实现] 灯光四类路由、速度/range-rate EMA、立即升级、迟滞降级、模式热切换和 policy 删除清理已经接通；`SlowMoving` 明确仍是 cache-on-dirty。
- [实现] caster 的平移/旋转/缩放检测、静态/overlay 迁移、Actor-root 后代继承和旧 `DynamicShadowCaster` 兼容已经接通。
- [实现] UE 导入 actor、PointLight mobility 默认映射、PerformanceLab、Map_S03B 蜡烛及飞行灯球 harness、HUD 分类统计已经接入；Map 专属代码只负责产生测试运动，不负责通用分类算法。
- [PC 验证] `cargo test --lib` 58/58；Map_S03B 8 秒截图无 panic/shader error，HUD 分类为灯 `2/16/0/2`、caster `43/2`。
- [Android 构建] Android Rust target 检查、release APK、4K 对齐和签名通过。
- [尚未 Android/VR 验证] ADB incremental install 读取 APK 时返回 `Bad file descriptor`。未验证自动迁移过程的双眼一致、旧影/漏光、动态分类长期稳定性和真机成本。

当前手动策略可由 Rust/ECS 在运行时设置；UE manifest 中可编辑的 per-light/per-actor policy 与阈值仍属于下一阶段。不能把 UE mobility 的默认映射误写成完整 authoring schema。

## 后续阶段

1. `SlowMoving` 双快照 atlas + cross-fade / shadow-term reconstruction；
2. priority、最大 stale time、GPU-ms 预算和 Hero 抢占；
3. dirty caster/light 的局部 face/page invalidation，替代全静态 atlas 失效；
4. UE schema 导出 per-light/per-actor policy、阈值、误差和优先级；
5. HUD policy 数量、迁移次数、stale time 与每类 GPU 成本；
6. Map_S03B 之外的合成 Static/Micro/Slow/FullyDynamic 压力场景和 16→32→64 斜率实验。
