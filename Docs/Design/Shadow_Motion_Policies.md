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
C_{policy}\approx O(I_sL_sFS)+O(I_mL_mFS)+O(U_kL_kFS)+O(L_fF(S+D))+O((L_s+L_m+L_k)FD)+O(P_t),
$$

其中：

- $I_s$：静态内容失效事件，稳定时趋近 0；
- $I_m$：微运动代理超界或静态内容变化事件，稳定时趋近 0；
- $U_k$：慢移/关键帧真实更新率；P2 由时间上限或投影位移误差触发，通常远小于每帧 1 次；
- 最后一项是 dynamic caster overlay，只绘制 $D$，不使静态层整层失效。
- $P_t$：当前正在 cross-fade 的灯覆盖的片元；这些片元增加一次旧静态 shadow sample，非过渡灯不支付该采样。

P1 的自动分类每帧每实体只做常数次向量/标量运算，CPU 成本为 $O(L+B)$，其中 $B$ 是带策略的 caster root 数。策略切换只在状态改变时增删 ECS 组件。

## 灯光策略

### 控制模式

- `Automatic`：引擎测量世界位移速度、range 变化率和虚拟原点 offset，并用迟滞选择 resolved class。
- `Static`、`BoundedMicroMotion`、`SlowMoving`、`FullyDynamic`：人工固定 resolved class；不会因观测结果自动改变。

### Resolved class 与当前路由

| Class | 持久 shadow cache | `PointLightShadowMapJitter` | 当前行为 |
| --- | --- | --- | --- |
| `Static` | 是 | 否 | 静态深度只在几何/投影失效时重画 |
| `BoundedMicroMotion` | 是 | 是 | 真实投影保持稳定，虚拟原点连续偏移 |
| `SlowMoving` | 是 | 否 | P2 低频真实快照；稀疏旧 cubemap 与新 cubemap 做 shadow-term cross-fade |
| `FullyDynamic` | 否 | 否 | 保留 Bevy 原生真实 moving-light redraw |

P2 已替换 P1 的 `SlowMoving cache-on-dirty`。稀疏 shadow pages、Hero priority、最大 stale time 和 GPU-ms 调度仍未实现，不能把当前固定槽池写成完整的预算调度器。

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

## P2：SlowMoving 稀疏双快照（2026-07-24）

### 数学与调度

每盏 `SlowMoving` PointLight 保存物理位置 $p(t)$、当前静态阴影快照位置 $s_1$ 和旧快照位置 $s_0$。只有满足以下任一条件且灯确实发生了投影变化时才请求新关键帧：

$$
\lVert p(t)-s_1\rVert\ge d_{max}
\quad\lor\quad
t-t_{snapshot}\ge \frac{1}{f_{snapshot}}.
$$

默认值是 $d_{max}=0.04\,m$、$f_{snapshot}=8\,Hz$。新快照开始前，将 resident static cubemap 的六层复制到稀疏槽；随后 resident cube 从真实新灯位重画。片元可见度为：

$$
V_{static}=\operatorname{mix}(V(s_0),V(s_1),\operatorname{smoothstep}(0,1,\tau)),
$$

$$
V=V_{static}\,V_{dynamic}(p(t)),
$$

其中 $\tau$ 默认在 0.12 秒内从 0 到 1。直接光照继续使用物理位置 $p(t)$；静态 shadow reconstruction 使用 $s_0/s_1$；动态 caster overlay 始终从真实灯位 $p(t)$ 更新。三者不能混为一个 Transform。

当过渡槽不足时，候选按 shadow snapshot 的 world-space stale error 从大到小排序，再以 Entity 作为稳定 tie-break；没有槽的灯保持旧快照等待，不会按眼睛、相机距离或随机数抢占。一个正在进行的过渡完成后才允许同一灯开始下一次快照，避免有限两状态在中途被第三状态覆盖。

`shadow_map_near_z` 改变会改变深度编码，旧/新图不能直接数值混合，因此当前实现执行一次真实重画并立即 settle。range/位置变化可以进入关键帧路径。运动超过 automatic slow threshold 时仍立即升级 `FullyDynamic`。

### 稀疏 atlas 与设备上限

设 shadowed PointLight 数为 $N$，设备最多允许 $A$ 个 texture-array layers；每 cube 占 6 layers。当前组合 atlas 为：

```text
[ N static cubes ][ N dynamic cubes ][ K_eff previous-snapshot cubes ]
```

$$
K_{eff}=\min\left(K_{config},\max\left(0,\left\lfloor\frac{A}{6}\right\rfloor-2N\right)\right).
$$

它不会为每盏灯永久分配第三份 cube。以 128×128 D32 为例，一份 PointLight cube 约为 $6\times128^2\times4=393216$ bytes（约 0.375 MiB）；4 个槽约 1.5 MiB。Map_S03B 当前运行时含 20 个 shadowed PointLight 时，256-layer 设备只能提供 2 个旧快照槽。RenderWorld 会把实际 $K_{eff}$ 通过共享原子状态反馈给主世界调度器，下一帧开始只分配设备真正可采样的槽。

shader 不再用 atlas 奇偶性猜布局；每个 GPU light record 携带 shadowed cube count、当前/旧 snapshot position、blend 和 slot。`GpuClusterableObject` 因此从 80 bytes 增至 112 bytes；16 KiB uniform fallback 上限从 204 个调整为 146 个 clusterable objects，仍高于当前 64-light 目标，但这是需要在 64/128-light 压力场景测量的明确 trade-off。

### 双眼与 RenderGraph 正确性

- snapshot 分类、槽、时间和 blend 只在主世界更新一次，左右眼共享同一组件数据。
- shadow node 可能为左右眼分别运行；cubemap copy 使用跨眼 `AtomicBool` claim，每帧每批 copy 只提交一次。否则第二只眼会把旧槽覆盖成已经重画的新 cube。
- shader 仅在 `transition_slot` 有效且小于 RenderWorld 的实际 pool size 时采样旧 cube；设备反馈延迟或运行时 atlas 变化不会导致越界采样。
- transition-only atlas 会把未使用的 dynamic layer 清为 fully lit；关闭 dynamic overlay 时不会重复提交 dynamic caster。

### 配置与遥测

`RenderQualityConfig` 新增：

- `slow_moving_shadow_crossfade`；
- `slow_moving_shadow_transition_slots`；
- `slow_moving_shadow_snapshot_hz`；
- `slow_moving_shadow_snapshot_distance_m`；
- `slow_moving_shadow_crossfade_seconds`。

HUD 显示 active/effective slots、waiting、snapshot starts、实际 cubemap copies 和最大 world-space stale distance。配置槽数只是请求值，HUD 的 effective slots 才是设备/当前灯数下可用值。

### P2 已验证与边界

- [实现] 通用主世界 scheduler、确定性稀疏槽池、RenderWorld capacity feedback、六层 texture copy、扩展 GPU ABI、shadow-term cross-fade、真实灯位 dynamic overlay 和 HUD telemetry。
- [PC 验证] 68 项 Rust 单元测试通过；Map_S03B 正常路径截图无 shader/Vulkan fatal；临时把两个测试飞行灯固定为 `SlowMoving` 后，HUD 显示 2 个 active cross-fade，GPU copy/渲染路径无 wgpu validation error。临时场景修改已恢复，产品源码没有 Map 专属分类。
- [Android 编译] default 与 `--no-default-features` 的 `aarch64-linux-android` 检查通过。
- [尚未 Android/VR 验证] PICO 上的视觉连续性、左右眼一致、实际有效槽反馈、GPU P50/P95/P99、20～30 分钟 thermal soak 尚未裁决；PC 无错误不等于移动端性能胜出。
- [边界] 当前 cross-fade 接入 Zevy scalable StandardMaterial direct-shadow 路径；stock volumetric-fog shadow sampling 尚未重建。SpotLight 仍只有真实单-frustum `FullyDynamic` 路径。
- [边界] 运行时增删 shadowed lights 导致 atlas 重排时，当前依赖下一帧 capacity feedback 与 cache 全失效；需要增加显式 atlas generation，确保正在过渡的旧槽不会跨 generation 使用。

P2 kill criterion：若真机观察到双影、半透明式 shadow 淡化不可接受、disocclusion 错误大于完整动态参考，或额外旧 shadow sample 的片元成本高于节省的六面重画，则按灯/材质回退 `FullyDynamic`，并进入 edge-aware reconstruction / temporal confidence，而不是缩短物理 range 或突然关影。

## 后续阶段

1. priority、最大 stale time、GPU-ms 预算和 Hero 抢占；
2. atlas generation、运行时增删灯安全迁移，以及旧/新 snapshot 的 edge-aware confidence reconstruction；
3. dirty caster/light 的局部 face/page invalidation，替代全静态 atlas 失效；
4. UE schema 导出 per-light/per-actor policy、阈值、误差和优先级；
5. 每类 shadow GPU timestamp、copy bytes、stale P50/P95/P99 和 thermal telemetry；
6. Map_S03B 之外的合成 Static/Micro/Slow/FullyDynamic 压力场景和 16→32→64 斜率实验。
