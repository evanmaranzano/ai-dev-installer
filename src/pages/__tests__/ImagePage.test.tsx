import "@testing-library/jest-dom/vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ImagePage } from "../ImagePage";

vi.mock("../../lib/image", () => ({
  generateImage: vi.fn()
}));

import { generateImage } from "../../lib/image";

const mockedGenerateImage = vi.mocked(generateImage);

test("shows prompt input and generate button", () => {
  render(<ImagePage />);

  expect(screen.getByRole("heading", { name: "生图", level: 1 })).toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "输入图片提示词" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "生成" })).toBeInTheDocument();
});

test("submits image request and renders returned image", async () => {
  const user = userEvent.setup();

  mockedGenerateImage.mockResolvedValueOnce({
    images: [
      {
        mimeType: "image/png",
        data: "ZmFrZS1pbWFnZQ=="
      }
    ]
  });

  render(<ImagePage defaultModel="gemini-2.0-flash-preview-image-generation" />);

  await user.type(screen.getByRole("textbox", { name: "输入图片提示词" }), "一只红色的猫");
  await user.click(screen.getByRole("button", { name: "生成" }));

  await waitFor(() => {
    expect(mockedGenerateImage).toHaveBeenCalledWith({
      model: "gemini-2.0-flash-preview-image-generation",
      prompt: "一只红色的猫",
      count: 1,
      aspectRatio: "1:1"
    });
  });

  expect(screen.getByRole("img", { name: "生成结果 1" })).toHaveAttribute(
    "src",
    "data:image/png;base64,ZmFrZS1pbWFnZQ=="
  );
});

test("does not render image payloads with unsupported mime types", async () => {
  const user = userEvent.setup();

  mockedGenerateImage.mockResolvedValueOnce({
    images: [
      {
        mimeType: "image/svg+xml",
        data: "PHN2ZyBvbmxvYWQ9YWxlcnQoMSk+PC9zdmc+"
      }
    ]
  });

  render(<ImagePage defaultModel="gemini-2.0-flash-preview-image-generation" />);

  await user.type(screen.getByRole("textbox", { name: "输入图片提示词" }), "一张图");
  await user.click(screen.getByRole("button", { name: "生成" }));

  await waitFor(() => {
    expect(mockedGenerateImage).toHaveBeenCalled();
  });

  expect(screen.queryByRole("img")).not.toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("已隐藏不支持的图片格式");
});

test("disables prompt input and generate button when actions are disabled", () => {
  render(<ImagePage actionsEnabled={false} />);

  expect(screen.getByRole("textbox", { name: "输入图片提示词" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "生成" })).toBeDisabled();
});

test("shows an error message when image generation fails", async () => {
  const user = userEvent.setup();

  mockedGenerateImage.mockRejectedValueOnce(new Error("network down"));

  render(<ImagePage />);

  await user.type(screen.getByRole("textbox", { name: "输入图片提示词" }), "一只红色的猫");
  await user.click(screen.getByRole("button", { name: "生成" }));

  await waitFor(() => {
    expect(screen.getByRole("alert")).toHaveTextContent("生成失败，请稍后重试。");
  });
});
