# Reduced-Rate Local Lighting 架构与实验计划

## 目标与当前证据

目标不是统一降低 XR render scale，而是把昂贵的动态局部光照和阴影采样从“每个完整分辨率片元都执行”改为“在可控的空间采样率上执行，再用几何边界感知重建”。几何、材质边缘、UI、中央注视区域和 Hero 交互对象仍可保持完整速率。

2026-07-24 在 PICO 设备 `PA9410MGJ9260457G`、ShadowMotionLab 16 灯、XR scale 1.0、MSAA 2x、相同 profiling APK 上得到以下稳定样本。这里的 GPU 是 PICO `PxrMetric` 整帧时间，不是 Bevy partial spans：

| 路径 | FPS avg | CPU avg / P95 | GPU avg / P95 | GPU 频率 |
|---|---:|---:|---:|---:|
| Geometry / post floor | 29.35 | 16.90 / 19.36 ms | 13.62 / 14.04 ms | 456 MHz |
| Direct only | 25.10 | 17.55 / 20.71 ms | 35.18 / 39.60 ms | 599 MHz |
| Shadow submission only | 33.10 | 14.83 / 17.74 ms | 12.60 / 13.13 ms | 456 MHz |
| Full direct + shadows | 17.95 | 16.04 / 19.19 ms | 52.07 / 55.14 ms | 599 MHz |

继续把 overflow 固定为预选 Top-K 后：

| 每片元完整 shadowed light 样本 | GPU avg / P95 @ 599 MHz |
|---:|---:|
| 4（2 Hero + 2 Tail） | 51.55 / 54.96 ms |
| 2（2 Hero） | 41.36 / 43.46 ms |
| 1（1 Hero） | 31.49 / 32.05 ms |

因此已被真机证伪的假设是：“只要把候选灯扫描改成固定 Top-K，就足以进入 72 Hz 预算。”即使 $K=1$，完整路径仍约 31.5 ms；选灯只改变灯数斜率，不能单独消除完整分辨率片元基数。

## 数学模型

当前 forward 路径可以粗略写为：

\[
C_{forward}\approx P(C_{material}+K(C_{BRDF}+C_{visibility}))+C_{shadow\_gen}+C_{fixed},
\]

其中 $P$ 是双眼实际着色片元，$K$ 是每片元执行完整 BRDF/阴影采样的灯数。当前 Top-K/cluster 工作主要压低 $K$，而 PICO 数据说明 $P$ 项仍是主导量级。

设局部光照只在比例 $r$ 的样本点执行，$r=1/2,1/4$ 分别代表约 2×1 与 2×2 采样；全分辨率重建成本为 $C_{reconstruct}$：

\[
C_{reduced}\approx P C_{gbuffer}+rPK(C_{BRDF}+C_{visibility})+PC_{reconstruct}+C_{shadow\_gen}+C_{fixed}.
\]

该方向只有在下式成立时才有结构性收益：

\[
(1-r)K(C_{BRDF}+C_{visibility}) > C_{gbuffer}+C_{reconstruct}+C_{attachment/barrier}.
\]

移动 tiler 上额外 attachment store/load 可能吃掉算术收益，因此必须分别记录 local-light pass、重建 pass、G-buffer 带宽和整帧 GPU；PC 结果不能裁决。

## 三条可回退路径

### 1. ForwardReference（已实现、默认）

现有 clustered forward + Zevy scalable PointLight + persistent shadow cache。它是画质参考和兼容 fallback，不按相机距离隐藏灯或扩大物理 range。

### 2. DeferredReference（已实现，PC 已验证）

`RenderQualityConfig.local_lighting_pipeline = LocalLightingPipeline::DeferredReference` 会：

- 将默认 opaque `StandardMaterial` 路由到 Bevy G-buffer；
- 自动给每个 3D camera 加 `DepthPrepass + DeferredPrepass`；
- 显式把有效 MSAA 设为 1x，因为 Bevy 0.16 deferred G-buffer 不支持 MSAA；
- 保留相同 Zevy 点灯、阴影 atlas、motion policy 和 shader 函数；
- 切回 Forward 时只恢复 Zevy 自己改过的 camera 组件，不删除其他系统原有 prepass。

它仍在全分辨率执行光照，不是性能胜出方案。PC ShadowMotionLab 16 已完成实际启动、shader 编译和截图；vendored Bevy deferred fullscreen vertex 已补 `@invariant`，消除 `CompareFunction::Equal` 的跨 GPU 精度风险。Android/VR 性能和双眼结果尚未验证。

### 3. ReducedRateDeferred（设计中，尚未实现）

首个产品候选应保留完整分辨率 G-buffer，但把局部直接光照写入每眼独立、低分辨率的 radiance buffer，再在 full-resolution composite 做几何边界感知上采样。不能直接把 2×2/4×4 raw lighting block 输出到眼睛。

建议最小数据流：

1. full-resolution depth、normal、base material/ID；
2. half/quarter-rate local direct-light + shadow visibility；
3. full-resolution bilateral reconstruction；
4. 与 emissive、IBL、雾、透明和后处理合成。

重建权重至少使用：

\[
w_i=w_d(\Delta z)w_n(n\cdot n_i)w_m(material\_id)w_s(\Delta screen),
\]

其中深度阈值必须按线性深度/局部 footprint 缩放，法线阈值保护折角，material ID 防止跨材质泄漏。低置信度像素应进入 full-rate edge fallback 或下一帧补样，不能接受漏光、阴影漂浮或双眼不一致。

## XR 与 foveation 约束

- 两眼共享灯候选、Top-K ID、shadow residency、随机 epoch、质量环带和调度历史；每眼仍使用自己的深度、世界位置和最终 radiance。
- 固定注视点可采用中心 full-rate、中环 half-rate、外围 quarter-rate；环带边界必须平滑并固定在统一 view geometry 中，不能产生头锁定亮度块。
- 眼动注视只有在 runtime 能力、预测、丢失 fallback 和双眼稳定全部验证后才能进入产品路径。
- 系统 FFR、OpenXR Fragment Density Map 与引擎自身 reduced-rate lighting 是三层不同机制，必须分别报告，不能把 vendor 属性值当成应用已经使用硬件 VRS 的证据。

当前已实现只读能力探针，区分：

1. OpenXR runtime advertised extensions；
2. Zevy instance enabled extensions；
3. PICO vendor-opaque `persist.pvr.foveation.level` 值。

本地源码审计表明 wgpu 24 尚未暴露 Vulkan Fragment Density Map。完整 `XR_FB_foveation_vulkan` 路径还需要 OpenXR swapchain create/enumerate next-chain、`VK_EXT_fragment_density_map` device/render-pass 支持以及 wgpu-hal/backend 修改。2026-07-24 旧设备日志曾显示 PICO system foveation level 12/event，但这只证明系统策略存在，不证明 Zevy swapchain 获得了 FDM。新探针 APK 已构建，因设备离线未安装验证。

## 实验顺序与 kill criteria

1. 在 Pico 上完成 ForwardReference 与 DeferredReference 四档成本分解；若 full-resolution deferred 的 G-buffer/带宽固定成本已明显劣于 forward，仍可作为低分辨率 substrate，但不能假定它免费。
2. 实现 quarter-rate diffuse-only local lighting，不含 specular/temporal，先证明 pass 真正按约 $rP$ 执行；保存 raw buffer 作为诊断。
3. 加 depth + normal + material-ID bilateral reconstruction，并用静态边缘、运动球、运动灯、薄几何和 disocclusion 夹具建立误差图。
4. 增加中央 full-rate / 外围 reduced-rate 环带，左右眼共享环带参数；测试转头和近距离灯影边界。
5. 再加入 temporal accumulation、specular tier、Hero edge fallback 和动态质量控制。
6. 与 OpenXR/PICO FFR 做正交 A/B；若硬件路径胜出且稳定，可 patch wgpu/OpenXR backend，不因工作量否决。

首个阶段通过门槛：同一 16 灯 fixture 下，相比 ForwardReference，PICO GPU P95 至少降低 25%，左右眼一致，灯/影不按相机距离 popping，边缘漏光和重建误差低于预先保存的阈值。若额外 G-buffer、attachment 和重建使 GPU P95 无显著改善，立即保留 reference path 并改走 tile-local/subpass、quad/subgroup 或 backend FDM，不继续堆叠无收益代码。
