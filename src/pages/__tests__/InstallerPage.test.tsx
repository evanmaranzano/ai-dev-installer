import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InstallerPage } from "../InstallerPage";

test("renders installer progress, component states, and logs", async () => {
  const user = userEvent.setup();
  const onInstallAll = vi.fn();
  const onRefreshSnapshot = vi.fn();

  render(
    <InstallerPage
      snapshot={{
        currentStage: "install_claude_code",
        progressPercent: 72,
        components: [
          { id: "git", label: "Git", status: "installed", detail: "已检测到 Git", version: "2.45.1" },
          {
            id: "python",
            label: "Python",
            status: "installed",
            detail: "Python 已安装",
            version: "Python 3.12.10"
          },
          {
            id: "nodejs",
            label: "Node.js",
            status: "installed",
            detail: "Node.js 已安装",
            version: "22.11.0"
          },
          {
            id: "cc_switch",
            label: "CC Switch",
            status: "installed",
            detail: "CC Switch 已安装",
            version: "1.2.0"
          },
          {
            id: "codex",
            label: "Codex",
            status: "installing",
            detail: "正在执行 npm install -g @openai/codex",
            version: null
          },
          {
            id: "claude_code",
            label: "Claude Code",
            status: "failed",
            detail: "Claude Code 安装失败",
            version: null
          }
        ],
        logs: [
          {
            timestamp: "10:00:00",
            stage: "install_claude_code",
            level: "info",
            message: "开始安装 Claude Code"
          }
        ],
        lastError: null
      }}
      isBusy={false}
      hasInitializationError={false}
      isRefreshingSnapshot={false}
      onInstallCodex={vi.fn()}
      onInstallClaudeCode={vi.fn()}
      onInstallAll={onInstallAll}
      onRefreshSnapshot={onRefreshSnapshot}
      onRetryStage={vi.fn()}
      onRetryAll={vi.fn()}
    />
  );

  expect(screen.getByRole("heading", { name: "AI Dev Installer", level: 1 })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "重新检测环境" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "安装 Codex" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "安装 Claude Code" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "全部安装" })).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "状态日志", level: 2 })).toBeInTheDocument();
  expect(screen.getByText("当前阶段：安装 Claude Code")).toBeInTheDocument();
  expect(screen.getByText("72%")).toBeInTheDocument();
  expect(screen.getByText("安装中")).toBeInTheDocument();
  expect(screen.getByText("正在执行 npm install -g @openai/codex")).toBeInTheDocument();
  expect(screen.getByText("Claude Code 安装失败")).toBeInTheDocument();
  expect(screen.getByText("[INSTALL_CLAUDE_CODE]")).toBeInTheDocument();
  expect(screen.getByText("INFO")).toBeInTheDocument();
  expect(screen.getByText("开始安装 Claude Code")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "全部安装" }));
  expect(onInstallAll).toHaveBeenCalledTimes(1);
  await user.click(screen.getByRole("button", { name: "重新检测环境" }));
  expect(onRefreshSnapshot).toHaveBeenCalledTimes(1);
});

test("renders failed retry state and initialization failure message", () => {
  const onRetryStage = vi.fn();
  const onRetryAll = vi.fn();

  render(
    <InstallerPage
      snapshot={{
        currentStage: "failed",
        progressPercent: 15,
        components: [
          {
            id: "git",
            label: "Git",
            status: "failed",
            detail: "初始化安装器桥接失败",
            version: null
          }
        ],
        logs: [
          {
            timestamp: "10:00:01",
            stage: "failed",
            level: "error",
            message: "初始化失败：loadInstallerSnapshot 调用失败"
          }
        ],
        lastError: "初始化失败：loadInstallerSnapshot 调用失败"
      }}
      isBusy={false}
      hasInitializationError
      isRefreshingSnapshot={false}
      onInstallCodex={vi.fn()}
      onInstallClaudeCode={vi.fn()}
      onInstallAll={vi.fn()}
      onRefreshSnapshot={vi.fn()}
      onRetryStage={onRetryStage}
      onRetryAll={onRetryAll}
    />
  );

  expect(screen.getByRole("heading", { name: "初始化失败", level: 2 })).toBeInTheDocument();
  expect(screen.getByRole("alert")).toHaveTextContent("初始化失败：loadInstallerSnapshot 调用失败");
  expect(screen.getByRole("button", { name: "安装 Codex" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "安装 Claude Code" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "全部安装" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "重试当前阶段" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "重新执行全部安装" })).toBeDisabled();
  expect(onRetryStage).not.toHaveBeenCalled();
  expect(onRetryAll).not.toHaveBeenCalled();
});

test("shows retry actions when the snapshot is failed", () => {
  render(
    <InstallerPage
      snapshot={{
        currentStage: "failed",
        progressPercent: 65,
        components: [],
        logs: [
          {
            timestamp: "2026-05-12T10:00:00Z",
            stage: "install_codex",
            level: "error",
            message: "npm install failed"
          }
        ],
        lastError: "npm install failed"
      }}
      isBusy={false}
      hasInitializationError={false}
      isRefreshingSnapshot={false}
      onInstallCodex={vi.fn()}
      onInstallClaudeCode={vi.fn()}
      onInstallAll={vi.fn()}
      onRefreshSnapshot={vi.fn()}
      onRetryStage={vi.fn()}
      onRetryAll={vi.fn()}
    />
  );

  expect(screen.getByRole("button", { name: "重试当前阶段" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "重新执行全部安装" })).toBeEnabled();
});

test("disables install and retry actions while snapshot refresh is running", () => {
  render(
    <InstallerPage
      snapshot={{
        currentStage: "failed",
        progressPercent: 65,
        components: [],
        logs: [],
        lastError: "install failed"
      }}
      isBusy={false}
      hasInitializationError={false}
      isRefreshingSnapshot
      onInstallCodex={vi.fn()}
      onInstallClaudeCode={vi.fn()}
      onInstallAll={vi.fn()}
      onRefreshSnapshot={vi.fn()}
      onRetryStage={vi.fn()}
      onRetryAll={vi.fn()}
    />
  );

  expect(screen.getByRole("button", { name: "刷新中..." })).toBeDisabled();
  expect(screen.getByRole("button", { name: "安装 Codex" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "安装 Claude Code" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "全部安装" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "重试当前阶段" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "重新执行全部安装" })).toBeDisabled();
});

test("renders only the latest 200 installer log entries", () => {
  const logs = Array.from({ length: 250 }, (_, index) => ({
    timestamp: "10:00:00",
    stage: "preflight" as const,
    level: "info" as const,
    message: `log ${index + 1}`
  }));

  render(
    <InstallerPage
      snapshot={{
        currentStage: "preflight",
        progressPercent: 10,
        components: [],
        logs,
        lastError: null
      }}
      isBusy={false}
      hasInitializationError={false}
      isRefreshingSnapshot={false}
      onInstallCodex={vi.fn()}
      onInstallClaudeCode={vi.fn()}
      onInstallAll={vi.fn()}
      onRefreshSnapshot={vi.fn()}
      onRetryStage={vi.fn()}
      onRetryAll={vi.fn()}
    />
  );

  expect(screen.getByText("已隐藏较早的 50 条日志。")).toBeInTheDocument();
  expect(screen.queryByText("log 50")).not.toBeInTheDocument();
  expect(screen.getByText("log 51")).toBeInTheDocument();
  expect(screen.getByText("log 250")).toBeInTheDocument();
  expect(screen.getAllByText("INFO")).toHaveLength(200);
});
