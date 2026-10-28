# 源码构建与开发验证

## 构建

当前源码已在 Windows MATLAB R2023a 构建；STM32 32 导 USB/BLE、打标器发送及硬件事件读取的短时实机验证使用旧 SDK 固定提交，详见 TRIGGER.md。本次 SDK 0.3.1 的 USB 采集验证见下方 SDK 升级验证。旧发布包仍仅含 ESP32 八导功能。

需要 MATLAB、Rust stable 和本机编译工具链。Rust 直接链接 MATLAB 的 MEX 库，无需 Python 或单独的 C++ MEX 编译器。SDK 由 Git 子模块固定提交 `54fbad405c05bfef813f7acb323e5f0c73153092`（`omnibci-sdk` 0.3.1），并启用 `esp32`、`e73-ble` 与 `trigger` feature；绑定自身不提升版本号。

```sh
git clone --recurse-submodules https://github.com/Omni-Intel/omnibci-matlab.git
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

`build_omnibci` 执行 `cargo build --offline --locked --release --features mex`，将 MEX 复制到 `matlab/+omnibci/private`。绑定依赖以 `features = ["esp32", "e73-ble", "trigger"]` 引入 SDK；`e73-ble` 隐含 `stm32`。修改 Rust 桥接代码后重新构建；Windows 上连接设备或扫描 BLE 后，MEX 会驻留到进程结束，避免后台清理线程执行已卸载代码；替换二进制前必须退出并重启 MATLAB，`clear mex` 不能释放该模块。仅运行离线解码时可使用 `clear mex`。构建产物不纳入 Git。

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

`test_hardware` 只覆盖原有 ESP32 八导路径，测试回写当前配置、连续读取 3 秒，检查数据形状、数量和停止后的状态。旧发布包在 Windows R2023a 的 ESP32 USB/BLE 短时采集曾通过；这不验证本源码适配，也不验证 STM32。STM32 32 导 USB/BLE 与打标的短时实机验证使用旧 SDK 固定提交，详见 TRIGGER.md；当前 SDK 的 USB 采集结果见下方。长时稳定性、掉线恢复、LOFF 关闭和 Linux/macOS 实机验收仍未完成。

## SDK 升级验证

2026-10-29 将 SDK 固定到 0.3.1 主线提交 `54fbad405c05bfef813f7acb323e5f0c73153092`，保留公开 MATLAB API 与绑定版本 0.2.1。此次升级不新增自动型号识别或 live-LOFF 的 MATLAB 参数。

Windows R2023a MEX 重新构建成功；27 个原生 Rust 测试、`cargo fmt --check` 与严格 Clippy 通过。重新构建后 `test_offline`、`test_stm32_offline`、`test_trigger_controller` 和 `test_trigger_timing` 通过，运行时版本为 SDK 0.3.1，revision 与子模块一致。独立进程的 `test_installation` 通过；Windows ZIP 构建后解压到独立目录，包内 MEX 的 selftest 与版本核对通过。固定 SDK 的 176 个单元测试和 10 个集成测试通过。

本次 MEX 在 COM25 STM32 32.3.0 USB 32 导与 COM8 ESP32 dual-stack_v0.25.4 USB 8 导分别完成连接、配置确认、至少 250 个样本读取、通道形状检查、停止和关闭；STM32 同时检查每样本 12 字节状态。另外，ESP32 0.26.1（44:B1:76:47:32:7A）和 STM32 32.3.0 / E73 0.6.0（CD:FB:E9:DC:82:6E）的 BLE 采集与立即清除 MEX 测试通过，进程正常退出。该短时验证不覆盖阻抗激励、触发器、掉线恢复或长时稳定性，也不代表 Linux/macOS 验收。旧 SDK 的触发测试结果保留在 TRIGGER.md。

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

## Windows MEX lifetime

The SDK may finish OS BLE cleanup after a bounded device close returns. The Windows MEX retains its module before starting connection or BLE discovery work using `GetModuleHandleExW(PIN | FROM_ADDRESS)` so `clear mex` and MATLAB shutdown cannot unmap active worker code. Device ownership is still released by explicit close; restarting MATLAB is required before replacing the binary after any connection attempt or BLE scan. This is a module lifetime guarantee, not a claim that OS cleanup always succeeds. See [Microsoft module lifetime documentation](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-getmodulehandleexw).

The opt-in `test_ble_lifetime(endpoint, deviceType)` exercises acquisition, clearing the MEX while connected, confirmed stop/close and immediate clearing again. Run it in a separate MATLAB process and verify that the process exits successfully; a printed pass line alone cannot detect an exit-time crash.

## Faulted LOFF cleanup

STM32/E73 faults with possible excitation now attempt a confirmed stop followed by a matching LOFF=0 configuration before disconnecting. Background cleanup is bounded to 10 seconds and close also respects its caller deadline. The original fault remains visible and the session cannot resume. Lost confirmations or a disconnected transport retain the unconfirmed-excitation diagnostic. Tests cover overflow, missing acknowledgements, stale acknowledgements arriving before/during cleanup, and bounded shutdown. Claude Code using DeepSeek deepseek-flash[1m] contributed implementation and review; Codex reviewed it and fixed the stale-ACK ordering boundary.

最终 BLE 复验曾出现一次 Windows ESP32 Not connected 连接错误；该进程正常以错误码退出。随后在新进程中重跑 ESP32 与 E73 的采集和立即清除 MEX 测试均通过，并确认进程退出码为 0。该记录保留连接瞬态限制，不将单次重试通过视为长时稳定性验收。
