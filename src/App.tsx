import { useEffect, useRef, useState } from "react";
import {
  loadInstallerSnapshot,
  listenInstallerSnapshot,
  refreshInstallerSnapshot,
  retryCurrentStage,
  retryInstallAll,
  startInstallFlow
} from "./lib/installer";
import type { InstallerSnapshot } from "./lib/types";
import { InstallerPage } from "./pages/InstallerPage";

const EMPTY_SNAPSHOT: InstallerSnapshot = {
  currentStage: "idle",
  progressPercent: 0,
  components: [
    { id: "git", label: "Git", status: "checking", detail: "等待检测", version: null },
    { id: "python", label: "Python", status: "checking", detail: "等待检测", version: null },
    { id: "nodejs", label: "Node.js", status: "checking", detail: "等待检测", version: null },
    { id: "cc_switch", label: "CC Switch", status: "checking", detail: "等待检测", version: null },
    { id: "codex", label: "Codex", status: "checking", detail: "等待检测", version: null },
    { id: "claude_code", label: "Claude Code", status: "checking", detail: "等待检测", version: null }
  ],
  logs: [],
  lastError: null
};

function isBusyStage(stage: InstallerSnapshot["currentStage"]) {
  return !["idle", "completed", "failed"].includes(stage);
}

function buildTimestamp() {
  return new Date().toLocaleTimeString("zh-CN", { hour12: false });
}

function toErrorMessage(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }

  if (
    error &&
    typeof error === "object" &&
    "message" in error &&
    typeof error.message === "string"
  ) {
    const details =
      "details" in error && typeof error.details === "string" && error.details.trim()
        ? ` ${error.details}`
        : "";
    return `${error.message}${details}`;
  }

  return String(error);
}

function createQueuedFlowSnapshot(
  snapshot: InstallerSnapshot,
  message: string
): InstallerSnapshot {
  return {
    ...snapshot,
    currentStage: "preflight",
    progressPercent: Math.max(snapshot.progressPercent, 1),
    lastError: null,
    components: snapshot.components.map((component) => ({
      ...component,
      status: "checking" as const,
      detail: "等待重新检测"
    })),
    logs: [
      ...snapshot.logs,
      {
        timestamp: buildTimestamp(),
        stage: "preflight",
        level: "info",
        message
      }
    ]
  };
}

function createInitializationFailureSnapshot(message: string): InstallerSnapshot {
  return {
    currentStage: "failed",
    progressPercent: 0,
    components: EMPTY_SNAPSHOT.components.map((component) => ({
      ...component,
      status: "failed",
      detail: "初始化安装器桥接失败"
    })),
    logs: [
      {
        timestamp: new Date().toLocaleTimeString("zh-CN", { hour12: false }),
        stage: "failed",
        level: "error",
        message
      }
    ],
    lastError: message
  };
}

export default function App() {
  const [snapshot, setSnapshot] = useState<InstallerSnapshot>(EMPTY_SNAPSHOT);
  const [isBusy, setIsBusy] = useState(false);
  const [hasInitializationError, setHasInitializationError] = useState(false);
  const [isRefreshingSnapshot, setIsRefreshingSnapshot] = useState(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    let unlisten: (() => void) | undefined;

    const failInitialization = (message: string) => {
      if (!mountedRef.current) {
        return;
      }

      setSnapshot(createInitializationFailureSnapshot(message));
      setIsBusy(false);
      setHasInitializationError(true);
    };

    void loadInstallerSnapshot()
      .then((value) => {
        if (mountedRef.current) {
          setSnapshot(value);
          setIsBusy(isBusyStage(value.currentStage));
          setHasInitializationError(false);
        }
      })
      .catch((error: unknown) => {
        failInitialization(`初始化失败：无法加载安装器状态。${toErrorMessage(error)}`);
      });

    void listenInstallerSnapshot((value) => {
      if (!mountedRef.current) {
        return;
      }

      setSnapshot(value);
      setIsBusy(isBusyStage(value.currentStage));
      setHasInitializationError(false);
    })
      .then((dispose) => {
        unlisten = dispose;
      })
      .catch((error: unknown) => {
        failInitialization(`初始化失败：无法订阅安装器状态更新。${toErrorMessage(error)}`);
      });

    return () => {
      mountedRef.current = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!isBusy || hasInitializationError) {
      return;
    }

    const timer = window.setInterval(() => {
      void loadInstallerSnapshot()
        .then((value) => {
          setSnapshot(value);
          setIsBusy(isBusyStage(value.currentStage));
          setHasInitializationError(false);
        })
        .catch(() => {
          // Ignore transient polling failures while the installer flow is still running.
        });
    }, 3000);

    return () => {
      window.clearInterval(timer);
    };
  }, [isBusy, hasInitializationError]);

  const handleRefreshSnapshot = () => {
    setIsRefreshingSnapshot(true);
    void refreshInstallerSnapshot()
      .then((value) => {
        if (!mountedRef.current) return;
        setSnapshot(value);
        setIsBusy(isBusyStage(value.currentStage));
        setHasInitializationError(false);
      })
      .catch((error: unknown) => {
        if (!mountedRef.current) return;
        setSnapshot(createInitializationFailureSnapshot(`初始化失败：无法重新检测环境。${toErrorMessage(error)}`));
        setIsBusy(false);
        setHasInitializationError(true);
      })
      .finally(() => {
        if (!mountedRef.current) return;
        setIsRefreshingSnapshot(false);
      });
  };

  const handleFlowStart = (flow: "install_codex" | "install_claude_code" | "install_all") => {
    const queuedMessage =
      flow === "install_all"
        ? "已提交全部安装请求，正在准备安装环境..."
        : flow === "install_claude_code"
          ? "已提交 Claude Code 安装请求，正在准备安装环境..."
          : "已提交 Codex 安装请求，正在准备安装环境...";

    setSnapshot((current) => createQueuedFlowSnapshot(current, queuedMessage));
    setIsBusy(true);
    setHasInitializationError(false);

    void startInstallFlow(flow).catch((error: unknown) => {
      const message = `启动安装失败：${toErrorMessage(error)}`;
      setSnapshot((current) => ({
        ...current,
        currentStage: "failed",
        lastError: message,
        logs: [
          ...current.logs,
          {
            timestamp: buildTimestamp(),
            stage: "failed",
            level: "error",
            message
          }
        ]
      }));
      setIsBusy(false);
    });
  };

  const handleRetryCurrentStage = () => {
    setSnapshot((current) =>
      createQueuedFlowSnapshot(current, "已提交重试请求，正在重新进入当前阶段...")
    );
    setIsBusy(true);
    setHasInitializationError(false);

    void retryCurrentStage().catch((error: unknown) => {
      const message = `重试当前阶段失败：${toErrorMessage(error)}`;
      setSnapshot((current) => ({
        ...current,
        currentStage: "failed",
        lastError: message,
        logs: [
          ...current.logs,
          {
            timestamp: buildTimestamp(),
            stage: "failed",
            level: "error",
            message
          }
        ]
      }));
      setIsBusy(false);
    });
  };

  const handleRetryAll = () => {
    setSnapshot((current) =>
      createQueuedFlowSnapshot(current, "已提交全部重试请求，正在重新准备安装流程...")
    );
    setIsBusy(true);
    setHasInitializationError(false);

    void retryInstallAll().catch((error: unknown) => {
      const message = `重新执行全部安装失败：${toErrorMessage(error)}`;
      setSnapshot((current) => ({
        ...current,
        currentStage: "failed",
        lastError: message,
        logs: [
          ...current.logs,
          {
            timestamp: buildTimestamp(),
            stage: "failed",
            level: "error",
            message
          }
        ]
      }));
      setIsBusy(false);
    });
  };

  return (
    <main
      style={{
        minHeight: "100vh",
        padding: "32px",
        fontFamily: "\"Segoe UI\", sans-serif",
        color: "#0f172a",
        background: "linear-gradient(180deg, #f8fafc 0%, #e0f2fe 100%)"
      }}
    >
      <div
        style={{
          maxWidth: "1120px",
          margin: "0 auto",
          padding: "28px",
          borderRadius: "24px",
          background: "rgba(255, 255, 255, 0.92)",
          boxShadow: "0 24px 80px rgba(15, 23, 42, 0.12)"
        }}
      >
        <InstallerPage
          snapshot={snapshot}
          isBusy={isBusy}
          isRefreshingSnapshot={isRefreshingSnapshot}
          hasInitializationError={hasInitializationError}
          onInstallCodex={() => handleFlowStart("install_codex")}
          onInstallClaudeCode={() => handleFlowStart("install_claude_code")}
          onInstallAll={() => handleFlowStart("install_all")}
          onRefreshSnapshot={handleRefreshSnapshot}
          onRetryStage={handleRetryCurrentStage}
          onRetryAll={handleRetryAll}
        />
      </div>
    </main>
  );
}
