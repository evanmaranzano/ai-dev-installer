import type { GeneratedImage } from "../lib/types";

interface ImageGridProps {
  images: GeneratedImage[];
}

const SAFE_IMAGE_MIME_TYPES = new Set(["image/png", "image/jpeg", "image/webp"]);

export function ImageGrid({ images }: ImageGridProps) {
  const safeImages = images.filter((image) => SAFE_IMAGE_MIME_TYPES.has(image.mimeType));
  const hiddenCount = images.length - safeImages.length;

  if (images.length === 0) {
    return <p style={{ margin: 0, color: "#6b7280" }}>暂时还没有生成结果。</p>;
  }

  return (
    <>
      {hiddenCount > 0 ? (
        <p role="status" style={{ margin: 0, color: "#92400e" }}>
          已隐藏不支持的图片格式。
        </p>
      ) : null}
      {safeImages.length > 0 ? (
        <div
          aria-label="图片结果网格"
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
            gap: "16px"
          }}
        >
          {safeImages.map((image, index) => (
            <figure
              key={`${image.mimeType}-${index}`}
              style={{
                margin: 0,
                padding: "12px",
                borderRadius: "16px",
                background: "#f8fafc"
              }}
            >
              <img
                alt={`生成结果 ${index + 1}`}
                src={`data:${image.mimeType};base64,${image.data}`}
                style={{ width: "100%", borderRadius: "12px", display: "block" }}
              />
            </figure>
          ))}
        </div>
      ) : null}
    </>
  );
}
