import type { InstallerLogEntry } from "../lib/types";

interface InstallerLogPanelProps {
  logs: InstallerLogEntry[];
}

const MAX_VISIBLE_LOGS = 200;

const STAGE_LABELS: Record<InstallerLogEntry["stage"], string> = {
  idle: "IDLE",
  preflight: "PREFLIGHT",
  install_git: "INSTALL_GIT",
  install_python: "INSTALL_PYTHON",
  install_node: "INSTALL_NODE",
  install_cc_switch: "INSTALL_CC_SWITCH",
  refresh_environment: "REFRESH_ENVIRONMENT",
  install_codex: "INSTALL_CODEX",
  install_claude_code: "INSTALL_CLAUDE_CODE",
  verify: "VERIFY",
  completed: "COMPLETED",
  failed: "FAILED"
};

const LEVEL_COLORS: Record<InstallerLogEntry["level"], string> = {
  info: "#0f172a",
  warn: "#b45309",
  error: "#b91c1c"
};

export function InstallerLogPanel({ logs }: InstallerLogPanelProps) {
  const visibleLogs = logs.slice(-MAX_VISIBLE_LOGS);
  const hiddenLogCount = logs.length - visibleLogs.length;

  return (
    <section
      style={{
        display: "grid",
        gap: "12px",
        padding: "20px",
        borderRadius: "20px",
        border: "1px solid #e2e8f0",
        background: "#ffffff"
      }}
    >
      <div>
        <h2 style={{ margin: 0 }}>状态日志</h2>
        <p style={{ margin: "8px 0 0", color: "#64748b" }}>
          记录安装流程中的检查、执行和错误信息。
        </p>
      </div>
      {logs.length === 0 ? (
        <p style={{ margin: 0, color: "#64748b" }}>暂无日志，开始安装后会在这里显示。</p>
      ) : (
        <>
          {hiddenLogCount > 0 && (
            <p style={{ margin: 0, color: "#64748b" }}>
              已隐藏较早的 {hiddenLogCount} 条日志。
            </p>
          )}
          <ul
            style={{
              display: "grid",
              gap: "10px",
              margin: 0,
              padding: 0,
              listStyle: "none"
            }}
          >
            {visibleLogs.map((entry, index) => (
              <li
                key={`${entry.timestamp}-${entry.stage}-${logs.length - visibleLogs.length + index}`}
                style={{
                  padding: "12px 14px",
                  borderRadius: "14px",
                  background: "#f8fafc",
                  color: LEVEL_COLORS[entry.level]
                }}
              >
                <div
                  style={{
                    display: "flex",
                    flexWrap: "wrap",
                    alignItems: "center",
                    gap: "8px"
                  }}
                >
                  <strong>{entry.timestamp}</strong>
                  <span
                    style={{
                      padding: "2px 8px",
                      borderRadius: "999px",
                      background: "rgba(15, 23, 42, 0.08)",
                      fontSize: "0.8rem",
                      fontWeight: 700,
                      letterSpacing: "0.03em"
                    }}
                  >
                    [{STAGE_LABELS[entry.stage]}]
                  </span>
                  <span
                    style={{
                      padding: "2px 8px",
                      borderRadius: "999px",
                      background: "rgba(255, 255, 255, 0.72)",
                      border: "1px solid rgba(15, 23, 42, 0.08)",
                      fontSize: "0.8rem",
                      fontWeight: 700,
                      letterSpacing: "0.03em"
                    }}
                  >
                    {entry.level.toUpperCase()}
                  </span>
                </div>
                <p style={{ margin: "6px 0 0" }}>{entry.message}</p>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
