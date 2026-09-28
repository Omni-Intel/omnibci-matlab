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
omnibci.Board.version()
```

发布包已包含对应平台 MEX，无需本地 Rust 编译。仓库的 `vX.Y.Z` 标签触发四个平台的 CI 编译与打包；只有全部成功后才创建 Release。手动运行同一工作流时，留空 `release_tag` 只生成 Actions artifact；填写已有版本标签则从该标签重新构建并发布。CI 访问私有 `omnibci-sdk` 子模块需要仓库 Secret `SUBMODULES_READ_TOKEN`。当前 CI 执行 Rust 测试与 MEX 编译；MATLAB 运行测试和设备测试仍需在有许可证的机器上分别验证。Windows USB/BLE 已完成本机验证，Linux 与 macOS 的硬件连接尚未验证。

## 构建

需要对应平台的 MATLAB、Rust stable 和本机编译工具链。Apple Silicon 原生 MATLAB 从 R2023b 开始提供，但当前 Mac 预编译包的最低版本为 R2025a；更早版本上的源码构建不在当前验证范围内。获取仓库时初始化 SDK 子模块：

```sh
git clone --recurse-submodules git@github.com:Omni-Intel/omnibci-matlab.git
```

在 MATLAB 中运行：

```matlab
cd('D:/workspace/omnibci-matlab')
build_omnibci
addpath(fullfile(pwd, 'matlab'))
addpath(fullfile(pwd, 'tests'))
test_offline
```

`build_omnibci` 使用 `cargo build --offline --locked --release --features mex`；首次构建如缺少 Cargo 依赖，请先在可联网环境运行 `cargo fetch --locked`。生成的 MEX 位于 `matlab/+omnibci/private`，不会被 Git 提交。修改 Rust 桥接代码后须重新运行构建，并在 MATLAB 中先 `clear mex`，再覆盖已有 MEX 文件。

## 连接和采集

```matlab
addpath('D:/workspace/omnibci-matlab/matlab')
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

## 验证

```matlab
addpath('D:/workspace/omnibci-matlab/matlab')
addpath('D:/workspace/omnibci-matlab/tests')
test_offline
```

Rust 可运行 `cargo test --locked` 和 `cargo clippy --locked -- -D warnings`。离线测试覆盖 MATLAB 到 Rust 的 MEX 调用、帧 CRC、符号扩展、增益缩放与无效 endpoint。本机已用目标硬件和固件完成 USB 与 BLE 的 3 秒采集验证；长时采集和停止尾包仍需验收。

本机连接板子后可运行 `test_hardware("usb")` 或 `test_hardware("ble")`。USB 脚本使用 COM8；BLE 从扫描结果中选择名为 `OmniBCI` 的设备。测试会回写当前配置、连续读取 3 秒，并检查数据形状、数量与停止后的状态。运行前应关闭其他占用设备的程序。

## 许可证

本仓库采用 BSD-3-Clause；SDK 子模块许可证见 `sdk/LICENSE`。
