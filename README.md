# OmniBCI MATLAB SDK

MATLAB 接口直接调用固定提交的 Rust `omnibci-sdk` 子模块，控制 ESP32-C3 + ADS1299 设备的 USB CDC / reliable BLE 采集。无需 Python 或单独的 C++ MEX 编译器；构建时 Rust 直接链接 MATLAB 提供的 MEX 库。

## 安装已发布版本

从 GitHub Releases 下载与系统匹配的 ZIP：

| ZIP 后缀 | MEX 扩展名 | 构建版本 | 预编译包最低 MATLAB 版本 | MATLAB 运行验证 |
| --- | --- | --- | --- | --- |
| `windows-x64` | `mexw64` | R2023a | R2023a | Windows R2023a：离线测试、USB/BLE 短时采集通过 |
| `linux-x64` | `mexa64` | R2023a | R2023a | 尚未验证；CI 仅完成 Rust 测试和 MEX 构建 |
| `macos-x64` | `mexmaci64` | R2025a | R2025a | 尚未验证；CI 仅完成 Rust 测试和 MEX 构建 |
| `macos-arm64` | `mexmaca64` | R2025a | R2025a | 尚未验证；CI 仅完成 Rust 测试和 MEX 构建 |

以上最低版本是预编译包的兼容性下限，不代表已验证其后的所有 MATLAB 或操作系统版本。MathWorks 建议使用与构建时相同的 MATLAB 版本；旧版构建的 MEX 通常可在新版运行，新版构建的 MEX 在旧版 MATLAB 上运行不受支持。参见 [MEX 版本兼容性](https://www.mathworks.com/help/matlab/matlab_external/version-compatibility.html)。

下载时应匹配正在运行的 MATLAB 架构，可用 `computer('arch')` 和 `mexext` 确认。Apple Silicon 机器上运行 Intel MATLAB 时应选 `macos-x64`；原生 Apple Silicon MATLAB 应选 `macos-arm64`。预编译包需要已安装并有许可证的 MATLAB。

解压后在对应平台的 MATLAB 中运行：

```matlab
addpath(fullfile('解压目录', 'omnibci-matlab-vX.Y.Z-平台后缀', 'matlab'))
omnibci.selftest()  % 不连接硬件；检查 MEX 加载、版本查询和离线解码
```

`selftest` 成功时打印通过信息，并返回 MATLAB 绑定和 Rust SDK 的版本。它覆盖完整帧解码、CRC 错误、符号扩展、增益缩放及无效 endpoint 拒绝；不扫描或连接设备，不替代硬件采集测试。`addpath` 仅对当前 MATLAB 会话生效，下次启动后需重新添加该目录。

安装失败时，根据异常标识处理：

| 异常标识 | 处理方法 |
| --- | --- |
| `omnibci:PlatformMismatch` | 包内只有其他平台的 MEX；按当前 MATLAB 的 `computer('arch')` 和 `mexext` 重新下载 |
| `omnibci:MissingBinary` | 安装目录缺少 MEX；重新下载并完整解压对应平台 ZIP |
| `omnibci:NotBuilt` | 当前使用源码仓库且尚未构建；按开发文档构建，或安装预编译 ZIP |
| `omnibci:NativeLoadFailed` | MEX 存在但无法加载；检查最低 MATLAB 版本及运行库，原始加载错误保留在异常中 |

发布包已包含对应平台 MEX，无需本地 Rust 编译。仓库的 `vX.Y.Z` 标签触发四个平台的 CI 编译与打包；只有全部成功后才创建 Release。手动运行同一工作流时，留空 `release_tag` 只生成 Actions artifact；填写已有版本标签则从该标签重新构建并发布。CI 访问私有 `omnibci-sdk` 子模块需要仓库 Secret `SUBMODULES_READ_TOKEN`。当前 CI 执行 Rust 测试与 MEX 编译；MATLAB 运行测试和设备测试仍需在有许可证的机器上分别验证。Windows USB/BLE 已完成本机验证，Linux 与 macOS 的硬件连接尚未验证。

## MATLAB 内置帮助

公开类、方法及函数的说明包含调用方式、参数默认值、返回字段、单位、异常和示例。添加安装目录中的 `matlab` 路径后可直接查询：

```matlab
help omnibci
help omnibci.Board
help omnibci.Board.connect
help omnibci.Board.read
help omnibci.decodeFrames
help omnibci.selftest
```

这些帮助注释与 `.m` 文件一起包含在 ZIP 中，不依赖在线文档或额外工具箱。

## Linux 运行依赖

Linux x64 预编译包在 Ubuntu 22.04 上构建。其他发行版尚未进行 MATLAB 运行验证，不能仅凭 x64 架构相同就认定兼容；系统还必须满足所用 MATLAB 版本的操作系统要求及 MEX 的动态库、glibc 符号版本要求。

当前 MEX 的直接动态库依赖为：MATLAB 提供的 `libmx.so`；系统提供的 `libudev.so.1`、`libdbus-1.so.3`、`libgcc_s.so.1`、`libm.so.6`、`libc.so.6` 和 `ld-linux-x86-64.so.2`。系统库不随 ZIP 分发。在 Ubuntu/Debian 上可安装运行依赖：

```sh
sudo apt-get update
sudo apt-get install libudev1 libdbus-1-3 libgcc-s1 libc6
```

`libmx.so` 应由目标机器的 MATLAB 安装提供，不要从构建机器复制 MATLAB 库到 SDK 目录。Linux 源码构建另需 `libudev-dev`、`libdbus-1-dev` 和 `pkg-config`；这些开发包不是使用预编译包的必要条件。

遇到 `Invalid MEX-file` 时，可对解压后的 `matlab/+omnibci/private/omnibci_mex.mexa64` 运行 `ldd` 检查缺失的系统库。普通终端中的 `ldd` 可能找不到 MATLAB 私有目录里的 `libmx.so`；应结合 MATLAB 内的实际加载结果判断。若出现 `GLIBC_x.y not found`，需要兼容的系统或在目标环境重新构建。

连接设备还要求当前用户可访问串口设备，BLE 要求可用的蓝牙适配器、BlueZ 服务及 D-Bus 访问权限。这些连接条件不等同于 MEX 加载依赖；无需连接硬件即可执行离线自检。

## 从源码构建

需要自行编译时，请克隆包含子模块的完整仓库，并按照 [源码构建与开发验证](https://github.com/Omni-Intel/omnibci-matlab/blob/master/DEVELOPMENT.md) 操作。预编译 ZIP 不包含 Rust 源码、构建脚本或仓库测试目录。

## 连接和采集

```matlab
ports = omnibci.Board.discover("serial")  % 返回 serial://... 字符串
ble = omnibci.Board.discover("ble", 10)     % 返回含 endpoint/name/RSSI 的结构

board = omnibci.Board.connect("serial://COM5");
cleanup = onCleanup(@() delete(board));
disp(board.snapshot());
board.start();
batch = board.read(2);     % 一批已接收数据，超时单位秒
size(batch.eeg_uv)         % N x 8，微伏，single
batch.sample_indices       % uint64 时间轴；缺失采样形成索引跳跃
board.stop();
board.close();
```

BLE 使用发现结果的 `endpoint`，不要自行把 MAC 地址当作 key。`connect` 会等设备配置读回；`start` 等首批数据；`stop` 保留尾部样本，可继续 `read`。发生超时、断线或缓冲溢出时抛出 `omnibci:Device` 异常。实时采集中要持续读取，以免 SDK 的有界队列溢出。设备只能由一个控制进程独占。

配置只能在 Ready 状态修改。先读取配置，修改字段再提交：

```matlab
config = board.getConfig();
config.gains = repmat(12, 1, 8);
board.configure(config);
assert(board.getConfig().verified);
```

`reference` 为 0(SRB1) 或 1(SRB2)；`mode` 为 0–4（both_bias、eeg、no_bias、shorted、test）；`enabled_mask`、`bias_mask`、`srb2_mask` 的 bit 0 对应第 1 通道；`gains` 允许 1、2、4、6、8、12、24。无效配置由 Rust SDK 拒绝。

`batch` 含 `eeg_uv`、`raw_counts`、`sequence`、`valid`、`mode`、`status`、`sample_indices`、`generation`、`sample_rate_hz`、`sample_time_s` 和 `received_age_s`。`received_age_s` 是从 SDK 最后一批主机接收时间到调用返回的经过时间，不是硬件采样时间。离线完整 USB 帧可用 `omnibci.decodeFrames(uint8(bytes), gains)` 解码；该函数不保留跨调用的不完整帧。

## 许可证

本项目采用 BSD-3-Clause，见 `LICENSE`。预编译 ZIP 内的 Rust SDK 许可证为 `SDK-LICENSE`；源码仓库中对应文件为 `sdk/LICENSE`。
