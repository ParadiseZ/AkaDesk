# AkaDesk `contracts` 接口设计

## 1. 文档目的

本文档用于定义 `desktop/host/contracts/` 这一层的抽象接口。

它解决的是：

- `contracts` 里到底放什么
- trait 应该怎么分模块
- trait 的输入输出模型按什么规则定义
- 哪些能力由 Runtime 编排，哪些只由 Adapter 实现
- `windows/linux` 该实现哪些接口

本文档不写实现代码，只定接口边界。

## 2. 文档位置与代码落位

本文档固定放在：

- `docs/07-contracts接口设计.md`

代码落位：

- 接口定义：`desktop/host/contracts/src/`
- 平台实现：`desktop/host/windows/src/`、`desktop/host/linux/src/`
- 编排消费：`desktop/host/runtime/src/`

当前目录建议：

```text
desktop/host/contracts/
  src/
    common/
    error/
    cap/
    audio/
    encode/
    display/
    input/
```

## 3. `contracts` 层职责

`contracts` 只负责三类东西：

1. trait
2. 共享领域模型
3. 共享错误类型

`contracts` 不负责：

- 平台 API 调用
- IPC 传输
- WebRTC 信令
- 会话状态机
- UI 展示态

一句话说清：

- `contracts = abstraction`
- `runtime = orchestration`
- `windows/linux = implementation`

## 4. 设计原则

### 4.1 不直接暴露 proto 消息

`contracts` 不直接把 Proto 生成类型当 trait 入参出参。

原因：

- Proto 是跨端传输模型
- `contracts` 是 Host 领域模型

所以：

- IPC / 控制层先把 Proto 消息映射成 Host 领域结构
- Runtime 再调用 `contracts` trait

### 4.2 trait 优先表达能力，不表达前端场景

例如：

- `DisplayControlPort`
- `AudioCapturePort`
- `VideoEncodePort`

而不是：

- `SettingsPagePort`
- `RemoteDesktopCardPort`

### 4.3 资源型能力必须有 prepare/start/stop 或 apply/restore 语义

因为 Host 这层的大部分能力不是纯查询，而是：

- 需要申请资源
- 需要持有资源
- 需要释放资源

### 4.4 Runtime 决策，Adapter 执行

例如视频：

- Runtime 决定是否尝试硬编、是否回退软编
- Adapter 只回答“这个实现能不能做”和“怎么做”

### 4.5 V1 先只把 H.264 走通

`encode` 抽象要预留多 codec，但 V1 正式落地只要求：

- `H.264`

其余 codec：

- 先保留枚举和能力结构
- 不要求第一版同步实现

## 5. 共享模型类别

`contracts` 里的共享模型建议只分五类：

| 类别 | 说明 |
| --- | --- |
| `Id` | 稳定标识，如 `DisplayId`、`SessionId` |
| `Snapshot` | 当前快照，如 `DisplaySnapshot`、`CapabilitySnapshot` |
| `Request` | 调用请求，如 `CaptureStartRequest` |
| `Result` | 调用结果，如 `DisplayApplyResult` |
| `Handle` | 资源句柄或会话句柄，如 `CaptureSessionHandle` |

不建议在 `contracts` 里放：

- UI Model
- Proto DTO
- Tauri command payload

## 6. 公共模块设计

## 6.1 `common/`

建议放：

- `SessionId`
- `DisplayId`
- `AudioDeviceId`
- `CaptureTargetId`
- `VideoCodec`
- `AudioCodec`
- `CodecImplementation`
- `TimestampMs`

还建议放这些通用结构：

- `CapabilitySnapshot`
- `DeviceRuntimeSnapshot`
- `HealthStatus`

## 6.2 `error/`

建议统一错误类型：

- `HostError`
- `HostErrorCode`
- `RetryClass`

建议错误分类至少覆盖：

- `InvalidRequest`
- `Unsupported`
- `Unavailable`
- `Timeout`
- `Conflict`
- `Internal`

约束：

- trait 层返回统一错误类型
- 具体平台错误在 impl 层归一化后再抛给 Runtime

## 7. `cap/` 接口设计

这里的 `cap` 解释为：

- 屏幕采集

而不是：

- 全部 capability 总称

## 7.1 模块职责

`cap/` 负责：

- 枚举可采集目标
- 查询采集能力
- 启动和停止屏幕采集
- 输出原始视频帧或帧流句柄

`cap/` 不负责：

- 视频编码
- WebRTC 发送
- UI 预览逻辑

Windows V1 采集策略建议固定为：

- 优先 `WGC`
- `WGC` 不可用或不稳定时回退 `DXGI`

## 7.2 建议 trait

### `CaptureQueryPort`

职责：

- 查询当前平台可采集目标
- 查询采集相关能力

建议方法语义：

- `list_capture_targets() -> Vec<CaptureTargetSnapshot>`
- `get_capture_capabilities(target_id) -> CaptureCapabilitySnapshot`

### `CaptureControlPort`

职责：

- 为会话准备采集资源
- 启动采集
- 停止采集

建议方法语义：

- `prepare_capture(request: CapturePrepareRequest) -> CapturePrepareResult`
- `start_capture(request: CaptureStartRequest) -> CaptureSessionHandle`
- `stop_capture(handle: CaptureSessionHandle) -> Result<()>`

## 7.3 关键共享模型

建议模型：

- `CaptureTargetSnapshot`
- `CaptureCapabilitySnapshot`
- `CapturePrepareRequest`
- `CapturePrepareResult`
- `CaptureStartRequest`
- `CaptureSessionHandle`
- `RawVideoFrame`

关键约束：

- `cap` 输出的是原始帧或原始帧流
- `runtime/video` 负责后续编排
- `encode` trait 不应直接反过来控制采集端

## 8. `audio/` 接口设计

## 8.1 模块职责

`audio/` 负责：

- 枚举系统音频设备
- 查询音频采集能力
- 启动和停止系统音频采集
- 输出 PCM 音频块或音频流句柄

`audio/` 不负责：

- Opus 编码决策
- WebRTC 音频发送

## 8.2 建议 trait

### `AudioDeviceQueryPort`

建议方法语义：

- `list_audio_devices() -> Vec<AudioDeviceSnapshot>`
- `get_default_output_device() -> Option<AudioDeviceId>`
- `get_audio_capabilities(device_id) -> AudioCapabilitySnapshot`

### `AudioCapturePort`

建议方法语义：

- `prepare_audio_capture(request: AudioPrepareRequest) -> AudioPrepareResult`
- `start_audio_capture(request: AudioStartRequest) -> AudioCaptureHandle`
- `stop_audio_capture(handle: AudioCaptureHandle) -> Result<()>`

## 8.3 关键共享模型

建议模型：

- `AudioDeviceSnapshot`
- `AudioCapabilitySnapshot`
- `AudioPrepareRequest`
- `AudioPrepareResult`
- `AudioStartRequest`
- `AudioCaptureHandle`
- `PcmAudioChunk`

关键约束：

- `audio` 输出 PCM
- `runtime/audio` 负责编排后续编码和发送

## 9. `encode/` 接口设计

## 9.1 模块职责

`encode/` 负责：

- 枚举可用视频编码实现
- 查询硬编/软编能力
- 启动、重配、停止编码会话
- 输出编码后的视频包或编码流句柄

`encode/` 不负责：

- 采集
- WebRTC 传输
- 选择是否回退

是否回退由：

- `runtime/video`

决定。

## 9.2 V1 视频口径

V1 正式要求：

- 只落 `H.264`

接口上预留：

- `HEVC`
- `VP8`
- `VP9`
- `AV1`

## 9.3 建议 trait

### `VideoEncoderQueryPort`

建议方法语义：

- `list_video_encoders() -> Vec<VideoEncoderDescriptor>`
- `get_video_encoder_capabilities(encoder_id) -> VideoEncoderCapabilitySnapshot`

### `VideoEncodePort`

建议方法语义：

- `prepare_video_encoder(request: VideoEncoderPrepareRequest) -> VideoEncoderPrepareResult`
- `start_video_encoder(request: VideoEncoderStartRequest) -> VideoEncoderHandle`
- `reconfigure_video_encoder(handle, request: VideoEncoderReconfigureRequest) -> Result<()>`
- `stop_video_encoder(handle) -> Result<()>`

## 9.4 关键共享模型

建议模型：

- `VideoEncoderDescriptor`
- `VideoEncoderCapabilitySnapshot`
- `VideoEncoderPrepareRequest`
- `VideoEncoderPrepareResult`
- `VideoEncoderStartRequest`
- `VideoEncoderReconfigureRequest`
- `VideoEncoderHandle`
- `EncodedVideoPacket`

关键约束：

- Runtime 决定优先硬编还是软编
- Adapter 只返回能力和执行结果
- 主控硬解失败后回退软解，是会话协商和 Runtime 策略问题，不属于 Host `encode` trait 的职责

## 10. `display/` 接口设计

## 10.1 模块职责

`display/` 负责：

- 枚举显示器
- 读取显示器当前快照
- 列出可用模式
- 应用显示器设置
- 在会话结束后恢复原设置

`display/` 不负责：

- 会话时长统计
- 会话状态机

## 10.2 建议 trait

### `DisplayQueryPort`

建议方法语义：

- `list_displays() -> Vec<DisplaySnapshot>`
- `get_display(display_id) -> DisplaySnapshot`
- `list_display_modes(display_id) -> Vec<DisplayMode>`

### `DisplayControlPort`

建议方法语义：

- `prepare_display(request: DisplayPrepareRequest) -> DisplayPrepareResult`
- `apply_display_settings(intent: DisplaySettingIntent) -> DisplayApplyResult`
- `restore_display_settings(token: DisplayRestoreToken) -> Result<()>`

## 10.3 关键共享模型

建议模型：

- `DisplaySnapshot`
- `DisplayMode`
- `DisplayPrepareRequest`
- `DisplayPrepareResult`
- `DisplaySettingIntent`
- `DisplayApplyResult`
- `DisplayRestoreToken`

关键约束：

- `apply` 和 `restore` 必须成对可追踪
- 显示器设置失败要能区分：
  - 可回退
  - 不可回退
- 会话中的分辨率修改，指修改目标显示器分辨率
- 分辨率修改允许按“重配编码器”或“重建采集+编码链路”处理
- `DisplaySettingIntent` 应显式包含 `requested_display_mode`
- `DisplaySettingIntent` 应显式包含 `history_display_mode`
- `history_display_mode` 必须在第一次显示器设置修改前记录
- 断开时必须恢复 `history_display_mode`
- 恢复内容至少包括：分辨率、缩放倍率、`fps/refresh_rate`

建议语义：

- 请求成功：
  - 当前显示器模式使用 `requested_display_mode`
- 请求失败：
  - 当前显示器模式保持 `history_display_mode`
  - 同时返回支持模式列表和失败原因

## 11. `input/` 接口设计

## 11.1 模块职责

`input/` 负责：

- 注入鼠标移动
- 注入鼠标按键
- 注入滚轮
- 注入键盘按键
- 为后续手势、触控、手柄输入预留能力扩展

`input/` 不负责：

- 输入事件协议解析
- 会话状态机
- UI 按键映射逻辑

## 11.2 建议 trait

### `InputCapabilityQueryPort`

建议方法语义：

- `get_input_capabilities() -> InputCapabilitySnapshot`

### `InputInjectionPort`

建议方法语义：

- `prepare_input(request: InputPrepareRequest) -> InputPrepareResult`
- `inject_pointer_event(event: PointerInputCommand) -> Result<()>`
- `inject_scroll_event(event: ScrollInputCommand) -> Result<()>`
- `inject_key_event(event: KeyInputCommand) -> Result<()>`
- `release_input(request: InputReleaseRequest) -> Result<()>`

## 11.3 关键共享模型

建议模型：

- `InputCapabilitySnapshot`
- `InputPrepareRequest`
- `InputPrepareResult`
- `InputReleaseRequest`
- `PointerInputCommand`
- `ScrollInputCommand`
- `KeyInputCommand`

关键约束：

- `input` trait 消费的是 Host 领域输入命令，不直接消费 Proto 消息
- 输入坐标继续统一用归一化坐标或已映射好的目标坐标
- Runtime 负责按 `session_id` 路由输入，不由 Adapter 感知多会话协议语义

## 12. Runtime 装配方式

`host-runtime` 不直接依赖平台实现细节，但会消费这些 trait。

建议由 Runtime 组装一个平台能力包：

- `CaptureQueryPort`
- `CaptureControlPort`
- `AudioDeviceQueryPort`
- `AudioCapturePort`
- `VideoEncoderQueryPort`
- `VideoEncodePort`
- `DisplayQueryPort`
- `DisplayControlPort`
- `InputCapabilityQueryPort`
- `InputInjectionPort`

可以概念上理解为：

- `PlatformPorts`

它由 `host-windows` 或 `host-linux` 提供具体实现。

## 13. 同步/异步边界

建议规则：

- 纯查询但可能触发系统调用的接口，也按异步设计
- 启动/停止/应用设置类接口，全部按异步设计
- 轻量纯内存 getter 不放在 trait 边界里

原因：

- Windows/Linux 平台调用经常涉及阻塞等待
- 后面无论接 Tokio 还是线程池都更稳

所以整体口径建议是：

- `contracts` trait 默认按 `async` 语义设计

## 14. 资源所有权与生命周期

资源型 trait 必须满足：

1. `prepare`
2. `start` 或 `apply`
3. `stop` 或 `restore`

不建议只有：

- `start`
- `stop`

而没有 `prepare`，因为：

- 会话协商时需要先知道“能不能做”
- UI 和 Runtime 需要在真正启动前拿到可诊断结果

## 15. V1 最小接口集合

如果只按 V1 最小闭环开工，第一批必须先定住的是：

1. `CaptureQueryPort`
2. `CaptureControlPort`
3. `AudioDeviceQueryPort`
4. `AudioCapturePort`
5. `VideoEncoderQueryPort`
6. `VideoEncodePort`
7. `DisplayQueryPort`
8. `DisplayControlPort`
9. `HostError`
10. `InputCapabilityQueryPort`
11. `InputInjectionPort`

## 16. 本次结论

这份 `contracts` 抽象接口设计文档固定放在：

- `docs/07-contracts接口设计.md`

它定的是：

- `contracts` 该放哪些 trait
- 每组 trait 的职责边界
- 输入输出模型按什么类别拆
- Runtime 应如何消费这些接口

下一步如果继续，最自然的动作就是把这里的最小接口集合和前面的数据结构文档一起收敛成真正的 Rust trait 与领域模型。  
