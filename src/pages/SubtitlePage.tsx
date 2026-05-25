import { useState } from "react";
import { extractSubtitles } from "../lib/subtitles";
import type { SubtitleSegment, TranscriptResult } from "../lib/types";

const MAX_SUBTITLE_UPLOAD_BYTES = 50 * 1024 * 1024;

interface SubtitlePageProps {
  actionsEnabled?: boolean;
  defaultModel?: string;
}

export function SubtitlePage({
  actionsEnabled = true,
  defaultModel = "gemini-2.0-flash"
}: SubtitlePageProps) {
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [result, setResult] = useState<TranscriptResult | null>(null);
  const [isExtracting, setIsExtracting] = useState(false);
  const [errorMessage, setErrorMessage] = useState("");

  function readFileBytes(file: File): Promise<number[]> {
    return new Promise((resolve, reject) => {
      const reader = new FileReader();

      reader.onload = () => {
        const result = reader.result;
        if (!(result instanceof ArrayBuffer)) {
          reject(new Error("invalid_file_buffer"));
          return;
        }

        resolve(Array.from(new Uint8Array(result)));
      };

      reader.onerror = () => {
        reject(reader.error ?? new Error("file_read_failed"));
      };

      reader.readAsArrayBuffer(file);
    });
  }

  async function handleExtract() {
    if (!actionsEnabled || !selectedFile) {
      return;
    }

    setIsExtracting(true);
    try {
      setErrorMessage("");
      if (selectedFile.size > MAX_SUBTITLE_UPLOAD_BYTES) {
        setErrorMessage("文件过大（超过 50MB），请选择更小的文件。");
        return;
      }
      const data = await readFileBytes(selectedFile);
      const response = await extractSubtitles({
        model: defaultModel,
        fileName: selectedFile.name,
        mimeType: selectedFile.type,
        data
      });

      setResult(response);
      setErrorMessage("");
    } catch {
      setErrorMessage("提取失败，请稍后重试。");
    } finally {
      setIsExtracting(false);
    }
  }

  return (
    <section style={{ display: "grid", gap: "20px" }}>
      <header>
        <h1 style={{ margin: 0 }}>字幕提取</h1>
        <p style={{ margin: "8px 0 0", color: "#4b5563" }}>
          通过本地 Rust command 上传文件、生成字幕并导出 SRT。
        </p>
      </header>

      <div
        style={{
          display: "grid",
          gap: "16px",
          padding: "20px",
          borderRadius: "16px",
          background: "#ffffff",
          boxShadow: "0 20px 50px rgba(15, 23, 42, 0.08)"
        }}
      >
        {errorMessage ? (
          <p
            role="alert"
            style={{
              margin: 0,
              padding: "12px 14px",
              borderRadius: "12px",
              background: "#fee2e2",
              color: "#991b1b"
            }}
          >
            {errorMessage}
          </p>
        ) : null}

        <label style={{ display: "grid", gap: "8px" }}>
          <span>选择音视频文件</span>
          <input
            aria-label="选择音视频文件"
            type="file"
            disabled={!actionsEnabled}
            onChange={(event) => {
              setErrorMessage("");
              setSelectedFile(event.target.files?.[0] ?? null);
            }}
          />
        </label>

        <button
          type="button"
          onClick={() => void handleExtract()}
          disabled={!actionsEnabled || isExtracting}
          style={{
            width: "fit-content",
            padding: "10px 18px",
            border: 0,
            borderRadius: "999px",
            background: "#0f172a",
            color: "#ffffff",
            fontWeight: 600
          }}
        >
          提取字幕
        </button>

        {result ? (
          <div style={{ display: "grid", gap: "12px" }}>
            <h2 style={{ margin: 0 }}>提取结果</h2>
            <ul
              aria-label="字幕片段列表"
              style={{ display: "grid", gap: "10px", padding: 0, margin: 0, listStyle: "none" }}
            >
              {result.segments.map((segment: SubtitleSegment, index: number) => (
                <li
                  key={`${segment.startMs}-${segment.endMs}-${index}`}
                  style={{
                    padding: "12px 14px",
                    borderRadius: "14px",
                    background: "#f8fafc"
                  }}
                >
                  <strong style={{ display: "block", marginBottom: "6px" }}>
                    {segment.startMs}ms - {segment.endMs}ms
                  </strong>
                  <span>{segment.text}</span>
                </li>
              ))}
            </ul>
            <p style={{ margin: 0 }}>
              导出路径：<span>{result.artifact.path}</span>
            </p>
          </div>
        ) : null}
      </div>
    </section>
  );
}
