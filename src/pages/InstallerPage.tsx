import { InstallerLogPanel } from "../components/InstallerLogPanel";
import { InstallerProgressPanel } from "../components/InstallerProgressPanel";
import { InstallerStatusCard } from "../components/InstallerStatusCard";
import type { InstallerSnapshot } from "../lib/types";
import gulouAiCenterImage from "../assets/gulou-ai-center.jpg";

interface InstallerPageProps {
  snapshot: InstallerSnapshot;
  isBusy: boolean;
  isRefreshingSnapshot: boolean;
  hasInitializationError: boolean;
  onInstallCodex: () => void;
  onInstallClaudeCode: () => void;
  onInstallAll: () => void;
  onRefreshSnapshot: () => void;
  onRetryStage: () => void;
  onRetryAll: () => void;
}

export function InstallerPage({
  snapshot,
  isBusy,
  isRefreshingSnapshot,
  hasInitializationError,
  onInstallCodex,
  onInstallClaudeCode,
  onInstallAll,
  onRefreshSnapshot,
  onRetryStage,
  onRetryAll
}: InstallerPageProps) {
  const areInstallActionsDisabled = isBusy || isRefreshingSnapshot || hasInitializationError;
  const areRetryActionsDisabled =
    isBusy || isRefreshingSnapshot || hasInitializationError || snapshot.currentStage !== "failed";

  return (
    <section style={{ display: "grid", gap: "24px" }}>
      <header
        style={{
          position: "relative",
          overflow: "hidden",
          borderRadius: "28px",
          minHeight: "240px",
          padding: "28px 32px",
          display: "grid",
          alignItems: "end",
          backgroundColor: "#0f172a",
          boxShadow: "0 20px 60px rgba(15, 23, 42, 0.18)"
        }}
      >
        <img
          src={gulouAiCenterImage}
          alt="福州市鼓楼区人工智能产业加速中心"
          style={{
            position: "absolute",
            inset: 0,
            width: "100%",
            height: "100%",
            objectFit: "cover",
            objectPosition: "center 34%"
          }}
        />
        <div
          aria-hidden="true"
          style={{
            position: "absolute",
            inset: 0,
            background:
              "linear-gradient(90deg, rgba(15, 23, 42, 0.88) 0%, rgba(15, 23, 42, 0.72) 36%, rgba(15, 23, 42, 0.34) 68%, rgba(15, 23, 42, 0.18) 100%)"
          }}
        />
        <div
          style={{
            position: "relative",
            zIndex: 1,
            maxWidth: "620px",
            display: "grid",
            gap: "14px"
          }}
        >
          <span
            style={{
              width: "fit-content",
              display: "inline-flex",
              alignItems: "center",
              padding: "8px 14px",
              borderRadius: "999px",
              background: "rgba(255, 255, 255, 0.16)",
              border: "1px solid rgba(255, 255, 255, 0.24)",
              color: "#e0f2fe",
              fontSize: "0.9rem",
              fontWeight: 700,
              letterSpacing: "0.02em",
              backdropFilter: "blur(10px)"
            }}
          >
            福州市鼓楼区人工智能产业加速中心
          </span>
          <div style={{ display: "grid", gap: "10px" }}>
            <h1 style={{ margin: 0, color: "#f8fafc", fontSize: "clamp(2rem, 4vw, 3.4rem)" }}>
              AI Dev Installer
            </h1>
            <p
              style={{
                margin: 0,
                color: "rgba(241, 245, 249, 0.92)",
                fontSize: "1rem",
                lineHeight: 1.7
              }}
            >
              面向 Windows 的 AI 开发环境一键部署器，一次准备 Git、Python、Node.js、CC Switch、
              Codex 与 Claude Code 所需基础环境。
            </p>
          </div>
        </div>
      </header>
      <InstallerProgressPanel snapshot={snapshot} hasInitializationError={hasInitializationError} />
      <section style={{ display: "grid", gap: "12px" }}>
        <h2 style={{ margin: 0 }}>组件状态</h2>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
            gap: "14px",
            alignItems: "stretch"
          }}
        >
          {snapshot.components.map((component) => (
            <InstallerStatusCard key={component.id} component={component} />
          ))}
        </div>
      </section>
      <div
        style={{
          display: "flex",
          gap: "12px",
          flexWrap: "wrap",
          alignItems: "center"
        }}
      >
        <button
          type="button"
          disabled={isRefreshingSnapshot}
          onClick={onRefreshSnapshot}
          style={{
            padding: "10px 14px",
            borderRadius: "12px",
            border: "1px solid #cbd5e1",
            background: "#f8fafc",
            color: "#0f172a",
            fontWeight: 600
          }}
        >
          {isRefreshingSnapshot ? "刷新中..." : isBusy ? "刷新当前状态" : "重新检测环境"}
        </button>
        <button type="button" disabled={areInstallActionsDisabled} onClick={onInstallCodex}>
          安装 Codex
        </button>
        <button type="button" disabled={areInstallActionsDisabled} onClick={onInstallClaudeCode}>
          安装 Claude Code
        </button>
        <button type="button" disabled={areInstallActionsDisabled} onClick={onInstallAll}>
          全部安装
        </button>
        <button
          type="button"
          disabled={areRetryActionsDisabled}
          onClick={onRetryStage}
        >
          重试当前阶段
        </button>
        <button
          type="button"
          disabled={areRetryActionsDisabled}
          onClick={onRetryAll}
        >
          重新执行全部安装
        </button>
      </div>
      <InstallerLogPanel logs={snapshot.logs} />
    </section>
  );
}
