# OmniBCI MATLAB SDK

通过 Rust `omnibci-sdk` 控制 ESP32-C3 + ADS1299 的 USB CDC / reliable BLE 采集。

## 安装

从 GitHub Releases 下载与 MATLAB 架构匹配的 ZIP；用 `computer('arch')` 和 `mexext` 确认架构。

| ZIP 后缀 | MEX 扩展名 | 构建及最低 MATLAB 版本 | 运行验证 |
| --- | --- | --- | --- |
| `windows-x64` | `mexw64` | R2023a | Windows R2023a 离线测试、USB/BLE 短时采集通过 |
| `linux-x64` | `mexa64` | R2023a | 尚未验证；CI 仅执行 Rust 检查和 MEX 构建 |
| `macos-x64` | `mexmaci64` | R2025a | 尚未验证；CI 仅执行 Rust 检查和 MEX 构建 |
| `macos-arm64` | `mexmaca64` | R2025a | 尚未验证；CI 仅执行 Rust 检查和 MEX 构建 |

Apple Silicon 上的 Intel MATLAB 使用 `macos-x64`，原生 MATLAB 使用 `macos-arm64`。需要已安装并有许可证的 MATLAB；最低版本以上的 MATLAB 和操作系统组合未逐一验证。

解压后运行：

```matlab
addpath(fullfile('解压目录', 'omnibci-matlab-vX.Y.Z-平台后缀', 'matlab'))
omnibci.selftest()
```

`selftest` 检查 MEX 加载、版本查询和离线解码，通过后打印结果并返回绑定与 SDK 版本，不连接硬件。`addpath` 仅对当前会话生效。

### 安装诊断

| 异常标识 | 处理方法 |
| --- | --- |
| `omnibci:PlatformMismatch` | 按当前 MATLAB 的 `computer('arch')` 和 `mexext` 重新下载 |
| `omnibci:MissingBinary` | 缺少 MEX；重新下载并完整解压 ZIP |
| `omnibci:NotBuilt` | 源码仓库尚未构建；按开发文档构建，或安装预编译 ZIP |
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

```matlab
ports = omnibci.Board.discover("serial")  % serial://... 字符串
ble = omnibci.Board.discover("ble", 10)   % endpoint/name/address/rssi_dbm 结构

board = omnibci.Board.connect("serial://COM5");  % 替换为设备端口
cleanup = onCleanup(@() delete(board));
disp(board.snapshot());

% 在 Ready 状态下配置
config = board.getConfig();
config.gains = repmat(12, 1, 8);
board.configure(config);
config = board.getConfig();
assert(config.verified);

board.start();
batch = board.read(2);     % 等待一批数据，超时 2 秒；不是采集 2 秒
size(batch.eeg_uv)         % N x 8，微伏，single
batch.sample_indices      % uint64 时间轴；缺失采样形成索引跳跃
board.stop();
board.close();
```

BLE 连接使用发现结果的 `endpoint`。设备只允许一个控制进程连接。`connect` 等待配置读回，`start` 等待首批数据；连续采集时须循环 `read`，避免 SDK 队列溢出。超时、断线和缓冲溢出抛出 `omnibci:Device`。

`stop` 保留尾部样本；需要这些数据时，在 `close` 或再次 `start` 前用 `read` 取出。队列读空后抛出超时异常。

配置字段：`reference` 为 0（SRB1）或 1（SRB2）；`mode` 为 0–4（both_bias、eeg、no_bias、shorted、test）；三个通道掩码的 bit 0 对应第 1 通道，`bias_mask` 和 `srb2_mask` 必须是 `enabled_mask` 的子集；`gains` 允许 1、2、4、6、8、12、24。

`help omnibci.Board.read` 列出批次字段、类型和单位。`received_age_s` 表示本批数据从 SDK 主机接收到处理读取请求时的经过时间，不是硬件采样时间。

离线 USB 帧用 `omnibci.decodeFrames(uint8(bytes), gains)` 解码；不完整尾帧不跨调用保留。

## API 帮助

```matlab
help omnibci
help omnibci.Board
help omnibci.Board.connect
help omnibci.Board.configure
help omnibci.Board.read
help omnibci.decodeFrames
help omnibci.selftest
```

## 源码构建

见 [源码构建与开发验证](https://github.com/Omni-Intel/omnibci-matlab/blob/master/DEVELOPMENT.md)。预编译 ZIP 包含 MEX，无需 Rust 或 C++ MEX 编译器；Rust 源码、构建脚本和仓库测试需从 Git 仓库获取。

## 许可证

BSD-3-Clause，见 `LICENSE`。Rust SDK 许可证位于发布包的 `SDK-LICENSE` 或源码仓库的 `sdk/LICENSE`。
