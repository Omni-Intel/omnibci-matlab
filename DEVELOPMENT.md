# 源码构建与开发验证

## 构建

`dev/32ch` 已在 Windows MATLAB R2023a 构建；STM32 32 导 USB/BLE、打标器发送及硬件事件读取的短时实机验证使用旧 SDK 固定提交，详见 TRIGGER.md。本次升级未重跑实机测试。旧发布包仍仅含 ESP32 八导功能。

需要 MATLAB、Rust stable 和本机编译工具链。Rust 直接链接 MATLAB 的 MEX 库，无需 Python 或单独的 C++ MEX 编译器。SDK 由 Git 子模块固定提交 `e1f25b3a6d272e5040929c53b331d45738ff6d73`（`omnibci-sdk` 0.2.0），并启用 `esp32`、`e73-ble` 与 `trigger` feature；绑定自身不提升版本号。

```sh
git clone --branch dev/32ch --recurse-submodules https://github.com/Omni-Intel/omnibci-matlab.git
cd omnibci-matlab
git submodule status sdk
cargo fetch --locked
```

`Cargo.lock` 由 Cargo 依据当前 manifest 解析，不应手工编辑。Linux 构建另需 `libudev-dev`、`libdbus-1-dev` 和 `pkg-config`（ESP32 BLE 与 E73 BLE 共用系统 BLE 后端）。各平台构建版本见 [README](README.md#安装)；更早 MATLAB 版本的源码构建未经验证。

在 MATLAB 中切换到仓库根目录，运行：

```matlab
clear mex
build_omnibci
addpath(fullfile(pwd, 'matlab'))
addpath(fullfile(pwd, 'tests'))
test_offline
test_stm32_offline
```

`build_omnibci` 执行 `cargo build --offline --locked --release --features mex`，将 MEX 复制到 `matlab/+omnibci/private`。绑定依赖以 `features = ["esp32", "e73-ble", "trigger"]` 引入 SDK；`e73-ble` 隐含 `stm32`。修改 Rust 桥接代码后重新构建；覆盖已加载的 MEX 前先执行 `clear mex`。构建产物不纳入 Git。

## 验证

本机已执行 Rust 构建、测试、Clippy 及 MATLAB 离线测试；实机边界见 TRIGGER.md。在仓库根目录运行 Rust 检查和构建：

```sh
cargo test --locked
cargo clippy --locked -- -D warnings
```

MATLAB 的 `test_offline` 覆盖 MEX 调用、版本查询、空批次、八导帧 CRC、符号扩展、增益缩放和无效 endpoint 拒绝。`test_stm32_offline` 只检查版本和 SDK revision、八导离线解码兼容性以及设备类型/endpoint 拒绝；它不打开串口或 BLE，因此不能验证 STM32 的 32 导数据或 12 字节状态。Rust 单元测试另外覆盖 STM32 配置与设备类型配对校验、ESP32 配置拒绝 STM32 专有字段、元数据组件字段、32 导批次的 12 个状态字节序列化、批次形状校验，以及 close 清理时 `Stm32Stopped(Err)`/`Event::Error` 的保留、`send_before` 在过期截止时间先尝试非阻塞入队、以及最终 Shutdown 升级等语义。以上均为软件测试，不构成 STM32/E73 实机验证。

安装诊断须使用独立 MATLAB 进程，避免已加载的 `omnibci.Board` 干扰隔离测试：

```sh
matlab -batch "addpath(fullfile(pwd, 'tests')); test_installation"
```

硬件测试前关闭占用设备的程序，在已添加 `matlab` 和 `tests` 路径的 MATLAB 中运行：

```matlab
test_hardware("usb")  % COM8；使用其他端口时修改 tests/test_hardware.m
test_hardware("ble")  % 扫描名为 OmniBCI 的 ESP32 设备
```

`test_hardware` 只覆盖原有 ESP32 八导路径，测试回写当前配置、连续读取 3 秒，检查数据形状、数量和停止后的状态。旧发布包在 Windows R2023a 的 ESP32 USB/BLE 短时采集曾通过；这不验证本源码适配，也不验证 STM32。STM32 32 导 USB/BLE 与打标的短时实机验证使用旧 SDK 固定提交，详见 TRIGGER.md；当前升级未重跑实机测试。长时稳定性、掉线恢复、LOFF 关闭和 Linux/macOS 实机验收仍未完成。

## SDK 升级验证

本次只更新 SDK 固定提交并重新构建，公开 MATLAB API 与绑定版本 0.2.1 不变，未连接硬件。Windows R2023a MEX 重新构建成功；`cargo test --locked` 的 21 个原生 Rust 测试通过；`test_offline`、`test_stm32_offline`、`test_trigger_controller` 和 `test_trigger_timing` 通过；`cargo fmt --check` 与 `cargo clippy --locked -- -D warnings` 通过。未使用新 SDK 重跑实机测试；旧 SDK 固定提交下的 STM32 32.2.3 短时实机结果见 TRIGGER.md，不能作为新提交的硬件证据。

## Serial trigger dispatch

```matlab
trigger = omnibci.TriggerController.connect("COM23", 1);
cleanup = onCleanup(@() delete(trigger));
trigger.send(7); % integer code 0..255; Rust SDK 10000 baud 8N1 protocol
trigger.close();
```

`availablePorts()` lists serial candidates; callers select the actual generator.
`isOpen()` reports port ownership. Close is idempotent; a write failure closes the
SDK port. Successful send is host dispatch, not device acknowledgement or a device
timestamp. Acquisition event reception is separate from this controller.
