import "@testing-library/jest-dom/vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SubtitlePage } from "../SubtitlePage";

vi.mock("../../lib/subtitles", () => ({
  extractSubtitles: vi.fn()
}));

import { extractSubtitles } from "../../lib/subtitles";

const mockedExtractSubtitles = vi.mocked(extractSubtitles);

function createFile(name: string, type: string, content: string) {
  return new File([content], name, { type });
}

test("shows file picker and extract controls", () => {
  render(<SubtitlePage />);

  expect(screen.getByRole("heading", { name: "字幕提取", level: 1 })).toBeInTheDocument();
  expect(screen.getByLabelText("选择音视频文件")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "提取字幕" })).toBeInTheDocument();
});

test("submits selected file through wrapper and renders segments and export path", async () => {
  const user = userEvent.setup();
  const file = createFile("sample.wav", "audio/wav", "hello");

  mockedExtractSubtitles.mockResolvedValueOnce({
    segments: [
      { startMs: 0, endMs: 1500, text: "你好" },
      { startMs: 1500, endMs: 3000, text: "世界" }
    ],
    artifact: {
      path: "C:/exports/sample.srt",
      kind: "srt"
    }
  });

  render(<SubtitlePage defaultModel="gemini-2.0-flash" />);

  const input = screen.getByLabelText("选择音视频文件") as HTMLInputElement;
  fireEvent.change(input, { target: { files: [file] } });
  await user.click(screen.getByRole("button", { name: "提取字幕" }));

  await waitFor(() => {
    expect(mockedExtractSubtitles).toHaveBeenCalledWith({
      model: "gemini-2.0-flash",
      fileName: "sample.wav",
      mimeType: "audio/wav",
      data: expect.any(Array)
    });
  });

  expect(screen.getByText("你好")).toBeInTheDocument();
  expect(screen.getByText("世界")).toBeInTheDocument();
  expect(screen.getByText("C:/exports/sample.srt")).toBeInTheDocument();
});

test("disables file picker and extract button when actions are disabled", () => {
  render(<SubtitlePage actionsEnabled={false} />);

  expect(screen.getByLabelText("选择音视频文件")).toBeDisabled();
  expect(screen.getByRole("button", { name: "提取字幕" })).toBeDisabled();
});

test("shows an error message when subtitle extraction fails", async () => {
  const user = userEvent.setup();
  const file = createFile("sample.wav", "audio/wav", "hello");

  mockedExtractSubtitles.mockRejectedValueOnce(new Error("network down"));

  render(<SubtitlePage />);

  const input = screen.getByLabelText("选择音视频文件") as HTMLInputElement;
  fireEvent.change(input, { target: { files: [file] } });
  await user.click(screen.getByRole("button", { name: "提取字幕" }));

  await waitFor(() => {
    expect(screen.getByRole("alert")).toHaveTextContent("提取失败，请稍后重试。");
  });
});

test("rejects files that are too large before reading them into memory", async () => {
  const user = userEvent.setup();
  mockedExtractSubtitles.mockClear();
  const file = new File(["x"], "large.wav", { type: "audio/wav" });
  Object.defineProperty(file, "size", { value: 51 * 1024 * 1024 });

  render(<SubtitlePage />);

  const input = screen.getByLabelText("选择音视频文件") as HTMLInputElement;
  fireEvent.change(input, { target: { files: [file] } });
  await user.click(screen.getByRole("button", { name: "提取字幕" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("文件过大（超过 50MB）");
  expect(mockedExtractSubtitles).not.toHaveBeenCalled();
});
