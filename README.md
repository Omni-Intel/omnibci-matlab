# OmniBCI MATLAB SDK

> **开发分支（`dev/32ch`）**：STM32 32 导与硬件触发功能需要从源码构建，旧 Release ZIP 仅支持 ESP32 八导。安装前请看下方验证状态，构建步骤见 [DEVELOPMENT](DEVELOPMENT.md)。

本分支源码通过 Rust `omnibci-sdk` 控制 ESP32-C3 + ADS1299 八导 USB CDC / reliable BLE，以及 STM32H563 + ADS1299 32 导的板端 USB CDC 或 E73 BLE 采集；STM32 固件已移除 ESB Dongle 和双模式切换。两种设备均为 250 Hz；STM32 固定 SRB1。

## 验证状态

| 基线 | 已验证范围 | 限制与证据 |
| --- | --- | --- |
| 旧 Release ZIP | Windows R2023a ESP32 USB/BLE 短时采集 | 不包含本分支 STM32 适配；其他平台见安装表 |
| 当前 `dev/32ch`，SDK `e1f25b3a6d272e5040929c53b331d45738ff6d73` | Windows R2023a MEX 构建、Rust 与 MATLAB 离线检查 | SDK 升级未重跑硬件，详见 [开发验证](DEVELOPMENT.md#sdk-升级验证) |
| 旧 SDK `aa9ab512ecf6243646fbbb23e9432e3c79eac1ec` | STM32 32.2.3 / E73 0.5.1 的 USB/BLE 短时采集与打标 | 历史结果不能验证当前 pin；版本、事件数与边界见 [TRIGGER](TRIGGER.md) |

当前源码的长时采集、掉线、LOFF 关闭及 Linux/macOS 实机验收仍需单独完成。安装产物的绑定版本 0.2.1 不能唯一标识 SDK；可用 `info = omnibci.Board.version()` 后检查 `info.sdk_revision`。旧 ZIP 可能没有该字段。

## 安装

### 旧版发布 ZIP（仅 ESP32 八导）

从 GitHub Releases 下载与 MATLAB 架构匹配的 ZIP；用 `computer('arch')` 和 `mexext` 确认架构。这些 ZIP 不含 STM32 适配，也不代表 `dev/32ch` 源码；旧包的 `omnibci.Board.version()` 返回绑定与 SDK 版本，可能没有本分支新增的 `sdk_revision` 字段。

| ZIP 后缀 | MEX 扩展名 | 构建及最低 MATLAB 版本 | 运行验证 |
| --- | --- | --- | --- |
| `windows-x64` | `mexw64` | R2023a | Windows R2023a 离线测试、ESP32 USB/BLE 短时采集通过 |
| `linux-x64` | `mexa64` | R2023a | 尚未验证；CI 仅执行 Rust 检查和 MEX 构建 |
| `macos-x64` | `mexmaci64` | R2025a | 尚未验证；CI 仅执行 Rust 检查和 MEX 构建 |
| `macos-arm64` | `mexmaca64` | R2025a | 尚未验证；CI 仅执行 Rust 检查和 MEX 构建 |

此表只描述旧发布包，当前源码验证范围见上方验证状态。

Apple Silicon 上的 Intel MATLAB 使用 `macos-x64`，原生 MATLAB 使用 `macos-arm64`。需要已安装并有许可证的 MATLAB；最低版本以上的 MATLAB 和操作系统组合未逐一验证。

解压后运行：

```matlab
addpath(fullfile('解压目录', 'omnibci-matlab-vX.Y.Z-平台后缀', 'matlab'))
omnibci.selftest()
```

`selftest` 检查 MEX 加载、版本查询和离线解码，通过后打印结果并返回绑定与 SDK 版本，不连接硬件；本分支还增加 SDK revision。`addpath` 仅对当前会话生效。

从当前源码构建 32 导 MEX 的步骤见 [源码构建与开发验证](DEVELOPMENT.md)。

### 安装诊断

| 异常标识 | 处理方法 |
| --- | --- |
| `omnibci:PlatformMismatch` | 按当前 MATLAB 的 `computer('arch')` 和 `mexext` 重新下载 |
| `omnibci:MissingBinary` | 缺少 MEX；重新下载并完整解压 ZIP |
| `omnibci:NotBuilt` | 源码仓库尚未构建；按开发文档构建。旧版 ESP32 八导路径可安装预编译 ZIP |
| `omnibci:NativeLoadFailed` | 检查最低 MATLAB 版本和运行库；异常中保留了原始加载错误 |

### Linux 依赖

Linux x64 包在 Ubuntu 22.04 上构建。目标系统须满足 MATLAB 的操作系统要求及 MEX 的动态库、glibc 符号版本要求。

MEX 依赖 MATLAB 提供的 `libmx.so`，以及系统提供的 `libudev.so.1`、`libdbus-1.so.3`、`libgcc_s.so.1`、`libm.so.6`、`libc.so.6` 和 `ld-linux-x86-64.so.2`。Ubuntu/Debian 运行依赖：

```sh
sudo apt-get update
sudo apt-get install libudev1 libdbus-1-3 libgcc-s1 libc6
```

遇到 `Invalid MEX-file`，对 `matlab/+omnibci/private/omnibci_mex.mexa64` 运行 `ldd` 检查缺失库。终端中的 `ldd` 可能找不到 MATLAB 私有目录内的 `libmx.so`，须结合 MATLAB 加载结果判断；该库应由本机 MATLAB 提供。遇到 `GLIBC_x.y not found`，换用兼容系统或在目标环境重新构建。

USB 连接需要串口访问权限；BLE 需要蓝牙适配器、BlueZ 服务和 D-Bus 访问权限。

## 连接和采集

设备类型通过第三个位置参数显式选择：`connect(endpoint, timeout, deviceType)` 和 `discover(transport, timeout, deviceType)`；`deviceType` 为 `"esp32"`（默认）或 `"stm32"`。原有的两个参数调用保持不变。连接后 `board.DeviceType` 以只读属性返回所选后端。

```matlab
% ESP32 八导（默认 esp32）
ports = omnibci.Board.discover("serial")            % serial://... 字符串
ble = omnibci.Board.discover("ble", 10)             % ESP32 BLE 结构
board = omnibci.Board.connect("serial://COM5");     % 替换为设备端口
cleanup = onCleanup(@() delete(board));
board.start();
batch = board.read(2);     % 等待一批数据，超时 2 秒；不是采集 2 秒
size(batch.eeg_uv)         % N x 8，微伏，single
board.stop();
board.close();
```

```matlab
% STM32 32 导：板端 USB CDC 使用 serial://；BLE 使用 E73 扫描结果
devices = omnibci.Board.discover("ble", 10, "stm32");
if isempty(devices)
    error("未发现 STM32/E73 BLE 设备");
end
board = omnibci.Board.connect(string(devices(1).endpoint), 20, "stm32");
cleanup = onCleanup(@() delete(board));
assert(board.DeviceType == "stm32");
config = board.getConfig();        % mode/enabled_mask/bias_mask/loff_mask/gains
config.gains = repmat(12, 1, 32);
board.configure(config);
config = board.getConfig();
assert(config.verified);
board.start();
batch = board.read(2);
size(batch.eeg_uv)         % N x 32，微伏，single
size(batch.status)         % N x 12，四个原始 ADS1299 状态字
board.stop();
board.close();
```

BLE 连接使用发现结果的 `endpoint`；当前 STM32/E73 固件广播名为 `OmniBCI-32CH`，SDK 也兼容旧名称 `OmniBCI32-BLE`。ESP32 和 STM32 使用不同 BLE 服务，`deviceType` 必须与发现时一致，不要把 MAC 地址当作 key。设备只允许一个控制进程连接。`connect` 等待配置确认。ESP32 的 `start` 等待进入流式采集；STM32 的 `start` 等待固件 START 确认（SDK 的 `Stm32Started` 结果），成功返回时首个样本不一定已经到达，随后用 `read` 取数据。连续采集时须循环 `read`，避免 SDK 队列溢出。超时、断线和缓冲溢出抛出 `omnibci:Device`。

STM32 的 E73 BLE 路径要求实际 TX/RX 均为 2M、MTU 至少 153，并运行 E73 SPI 桥接固件；链路自检镜像不能完成 STM32 配置。STM32 固件须支持 32.0.5 系列的 AE/BE 完整配置事务，32.0.4 不兼容。当前固件仅提供板端 USB CDC 与 E73 BLE 直连，已移除 ESB Dongle 和双模式切换；本绑定不自动重连。

`stop` 保留尾部样本；需要这些数据时，在 `close` 或再次 `start` 前用 `read` 取出。队列读空后抛出超时异常。STM32 接口不自动重连；断线后需重新连接。

### ESP32 配置

`reference` 为 0（SRB1）或 1（SRB2）；`mode` 为 0–4（both_bias、eeg、no_bias、shorted、test）；三个通道掩码的 bit 0 对应第 1 通道，`bias_mask` 和 `srb2_mask` 必须是 `enabled_mask` 的子集；`gains` 允许 1、2、4、6、8、12、24。

### STM32 配置

字段为数值 `mode`（0 both_bias、1 eeg、2 no_bias、3 shorted、4 test）、`enabled_mask`、`bias_mask`、`loff_mask` 和 32 个 `gains`。三个掩码均为 0–4294967295 的 JSON 整数，bit 0 对应 CH1；`bias_mask` 和 `loff_mask` 必须是 `enabled_mask` 的子集，`loff_mask` 只允许在模式 0–2 下非零。增益允许 1、2、4、6、8、12、24。STM32 固定 250 Hz 和 SRB1，不提供 SRB2 或采样率字段；传错设备类型的配置会抛出 `omnibci:Configuration`。

只在停采时配置并等待 STM32 执行回执。`loff_mask` 是导联脱落激励配置，不是阻抗数值 API。关闭设备时，若 `loff_mask` 非零，`close` 会先停采并提交 `loff_mask=0`，只有 SDK 确认后才成功返回；若停止或关闭 LOFF 无法确认，会抛出 `omnibci:Device`，此时不得假定电极激励已关闭。Disconnect 停止失败时会在有限宽限期后升级为 Shutdown 尽力清理，清理失败同样会报告。`delete` 会抑制这些清理错误，需要观察时请显式调用 `close`。

`help omnibci.Board.read` 列出批次字段、类型和单位。`channel_count` 为 8（ESP32）或 32（STM32），`status_bytes` 为 3（ESP32）或 12（STM32，四个原始 ADS1299 状态字）。STM32 的 `status` 直接传递 V3 帧字节 12–23，不做重新解释；空批次仍保留 32 列通道和 12 列状态。`received_age_s` 表示本批数据从 SDK 主机接收到处理读取请求时的经过时间，不是硬件采样时间。

快照的 `metadata.stm32` / `metadata.e73` 分别给出组件的 `version`、`revision`、`dirty`、`state` 和 `protocol`；不可用时为空（JSON null）。原有 `firmware`、`hardware`、`protocol` 字段继续保留。`state` 使用 `Connecting`、`Ready`、`Starting`、`Streaming`、`Stopping`、`Closed`、`Faulted` 等 PascalCase 名称，计数器以十进制字符串返回以保留 uint64 精度。STM32 不提供 BLE 传输计数，快照的 `ble` 字段为空对象（不可用），而不是伪造的零值。

离线 USB 帧仍用 `omnibci.decodeFrames(uint8(bytes), gains)` 解码，该函数只支持 ESP32 八导协议；不完整尾帧不跨调用保留。STM32 V3 的离线解码不在本版 MATLAB API 内。

## API 帮助

```matlab
help omnibci
help omnibci.Board
help omnibci.Board.connect
help omnibci.Board.configure
help omnibci.Board.read
help omnibci.Board.readTriggers
help omnibci.TriggerController
help omnibci.TriggerController.connect
help omnibci.decodeFrames
help omnibci.decodeTriggers
help omnibci.selftest
```

源码仓库的 `examples/acquire.m` 展示 ESP32 采集；`examples/acquire_stm32.m` 是一个函数，使用显式 `"stm32"` 设备类型和 `onCleanup` 展示 32 导采集。传入 serial 或 ble endpoint，例如 `acquire_stm32("serial://COM5")`；不传参数时扫描 E73 BLE。函数作用域保证出错展开时 `onCleanup` 会销毁并关闭板卡。

## 源码构建

请按 [源码构建与开发验证](DEVELOPMENT.md) 使用 `--branch dev/32ch` 克隆并自行构建；设备支持和验证范围见本页开头。

## 文档

- [源码构建与开发验证](DEVELOPMENT.md)：工具链、MEX 构建和软件/硬件检查边界。
- [触发发送与接收](TRIGGER.md)：独立事件队列、精确整数时间、双队列轮询及历史实测。
- [SDK 兼容性判断](https://github.com/Omni-Intel/omnibci-sdk/blob/master/docs/compatibility.md)：设备、固件与调用方基线；发布复现应核对实际 SDK pin。
