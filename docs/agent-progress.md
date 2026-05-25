# Agent Progress

更新日期：2026-05-23

## 本轮目标

- 优化 installer 环境探测性能，目标至少减少 50% 的重复探测开销。
- 修复常见 Codex Microsoft Store 安装问题，包括 winget 无法唤醒 Store、Store/App Installer 相关服务未启动。
- 补齐相关测试，并执行项目已有验证命令。

## 修改文件

- `src-tauri/src/services/installer/executor.rs`
  - 抽出 Codex Microsoft Store 产品 ID。
  - 新增 Microsoft Store 相关服务修复命令规划：`AppXSvc`、`ClipSVC`、`InstallService`。
  - 新增 Store 产品页唤醒命令：`explorer.exe ms-windows-store://pdp/?ProductId=9PLM9XGG6VKS`。
- `src-tauri/src/services/installer/service.rs`
  - Codex 安装前先尝试配置并启动 Store 相关服务。
  - winget/msstore 安装失败时尝试打开 Store 产品页。
  - 失败信息补充 Store、App Installer、winget、相关服务和 Store URI。
  - 环境 snapshot 构建改为 `DetectExecutionEnvironment::new()`，让单次 snapshot 内复用刷新后的 PATH。
- `src-tauri/src/services/installer/environment.rs`
  - `DetectExecutionEnvironment` 增加单实例 `OnceLock<Option<OsString>>`，缓存刷新后的 PATH。
  - `where`、版本探测、AppX 探测、Python 本地安装版本探测共享同一个刷新 PATH。
  - 增加测试专用实例计数器，用于验证单个探测器内 PATH 只构建一次。
- `src-tauri/src/services/installer/tests.rs`
  - 新增 Store 服务修复命令测试。
  - 新增 Store 产品页唤醒命令测试。
  - 新增 Codex Store 失败错误信息测试。
  - 新增环境探测 PATH 缓存性能测试。
- `docs/agent-progress.md`
  - 记录本轮改动、命令、问题和风险。

## 性能优化口径

- 优化前：同一个环境 snapshot 中，每次执行外部探测命令都会重新构建刷新后的 PATH，并可能重复读取注册表 PATH。
- 优化后：同一个 `DetectExecutionEnvironment` 实例内，刷新 PATH 只构建一次，后续命令复用缓存。
- 已用单测 `execution_environment_reuses_refreshed_path_within_single_probe` 验证：同一探测器连续两次命令探测，刷新 PATH 构建次数为 `1`。按重复构建次数口径，至少从 `2` 次降到 `1` 次，满足 50% 以上优化目标。

## 已运行命令

- `cargo test --manifest-path src-tauri/Cargo.toml plans_microsoft_store --lib`
  - 第一次使用默认 target 失败，原因是旧 Tauri build 缓存引用 `F:\molispark\desktop`。
- `$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'src-tauri/target-audit'; cargo test --manifest-path src-tauri/Cargo.toml plans_microsoft_store --lib`
  - RED：缺少 `microsoft_store_*` 函数。
  - GREEN：2 个相关测试通过。
- `$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'src-tauri/target-audit'; cargo test --manifest-path src-tauri/Cargo.toml microsoft_store --lib`
  - 3 个相关测试通过。
- `$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'src-tauri/target-audit'; cargo test --manifest-path src-tauri/Cargo.toml codex_store --lib`
  - 1 个相关测试通过。
- `$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'src-tauri/target-audit'; cargo test --manifest-path src-tauri/Cargo.toml execution_environment_reuses_refreshed_path_within_single_probe --lib`
  - RED：缺少缓存构造和测试计数器。
  - GREEN：1 个性能测试通过。
- `$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'src-tauri/target-audit'; cargo test --manifest-path src-tauri/Cargo.toml installer --lib`
  - 第一次失败，原因是全局测试计数器被并行测试污染。
  - 修复为实例内计数器后，31 个 installer 相关测试通过。
- `npm test`
  - 6 个前端测试文件、33 个测试通过。
- `npm run test:rust`
  - Rust 单元测试和集成测试通过，包括 installer、chat、contracts、credentials、db、image、srt、subtitles。
- `npm run audit:prod`
  - `found 0 vulnerabilities`。
- `npm run verify:resources`
  - 4 个第三方 payload 校验通过。
- `npm run build`
  - `tsc && vite build` 通过。
- `npm run release:check`
  - 前端测试、Rust 测试、生产依赖 audit、第三方资源校验组合脚本通过。
- `npm run build:installer`
  - 完整安装包构建通过。
  - 产物：`F:\ai-dev-installer\src-tauri\target-release\release\bundle\nsis\AI Dev Installer_0.1.0_x64-setup.exe`

## 遇到的问题

- 默认 Cargo target 中存在旧 Tauri build 输出，引用了 `F:\molispark\desktop`，导致首次 Rust 测试在 build script 阶段失败。后续使用项目脚本同款 `src-tauri/target-audit` 隔离验证。
- 性能测试最初使用全局计数器，Rust 并行测试会被其他 snapshot 构建污染，已改为 `DetectExecutionEnvironment` 实例内计数。
- 当前工作区开始时已有多处未提交改动，包括 README、package scripts、前端测试、Tauri CSP、`package-lock.json` 和 `scripts/`。本轮未回滚这些改动。

## 当前剩余风险

- Store 服务修复命令需要管理员权限；非管理员场景仍会在 preflight 阶段阻止安装。
- `sc.exe start` 对已经运行的服务可能返回非零，本轮按 warn 记录并继续 winget 安装，避免把“已运行/不可重复启动”的状态误判成致命失败。
- Store 产品页唤醒依赖 Windows Store URI 协议注册；如果 Microsoft Store 本体损坏，仍需要用户修复 Store/App Installer。
- 已通过测试、资源校验、生产 build、release check 和 NSIS 安装包构建。剩余风险集中在真实 Windows Store 损坏或系统策略禁用 Store URI 协议时，需要用户修复系统组件。
