# AI Dev Installer

Windows 桌面 AI 开发环境安装器。React/TypeScript + Tauri 2/Rust + NSIS。

## 目录结构
```
src/                 — React 前端
├── components/      — UI 组件（ApiKeyCard, InstallerProgress 等）
├── pages/           — 页面（Chat, Image, Installer, Settings, Subtitle）
├── lib/             — 业务逻辑（chat, installer, settings, tauri bridge）
├── App.tsx          — 根组件
└── routes.tsx       — 路由
src-tauri/           — Rust 后端
├── src/commands/    — Tauri 命令（chat, installer, settings 等）
├── src/services/    — 业务服务（gemini, installer, credentials 等）
├── src/storage/     — SQLite 存储（db, history）
└── tests/           — Rust 集成测试
scripts/             — 构建脚本（含 test-rust.ps1）
```

## 技术栈
- 前端: React 18 + TypeScript + Vite
- 桌面: Tauri 2 (Rust crate `ai_dev_installer`)
- 安装器: NSIS
- 测试: Vitest (前端) + cargo test (Rust)

## 常用命令
```bash
npm run dev              # Vite 开发服务器
npm run build            # tsc + vite build
npm test                 # Vitest 前端测试
npm run test:rust        # Rust 测试 (scripts/test-rust.ps1)
npm run tauri dev        # Tauri 开发模式
npm run tauri build      # Tauri 生产构建
npm run release:check    # 全量检查: test + test:rust + audit + verify:resources
npm run build:installer  # NSIS 安装器构建
```

## Rust 测试
```bash
CARGO_TARGET_DIR=src-tauri/target-test cargo test --manifest-path src-tauri/Cargo.toml
```

## 版本号同步
三处必须同步: `package.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`

## 发布流程
版本号 → `npm run tauri build` → commit/tag/push → `gh release create`
未授权不执行 git 写操作。

## Release 产物
NSIS 安装器: `src-tauri/target/release/bundle/nsis/AI Dev Installer_<version>_x64-setup.exe`
`gh release create v<ver> "./src-tauri/target/release/bundle/nsis/AI Dev Installer_<ver>_x64-setup.exe" --title "v<ver>" --notes "..."`

## 数据路径
- 设置: `%APPDATA%/AI Dev Installer/settings.json`
- 历史: `%LOCALAPPDATA%/AI Dev Installer/history.sqlite3`

## Rust 注意事项
- Windows `Command::output()` 无超时，sc.exe 等可能永久阻塞；用 `child.try_wait()` + deadline + `child.kill()`
- inherent/trait 同名方法: inherent 会 shadow trait；trait impl 内 `self.foo()` 可能因删除 inherent 后变无限递归，改名如 `foo_impl`
- 重试/re-verify 必须有上限，如 `max_retries=10`
- 函数返回值必须检查后再标记完成，不能无条件 mark_done
