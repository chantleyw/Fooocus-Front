import { useState } from "react";
import { Images } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import { errorMessage, type GalleryImage } from "../lib/api";
import { Chip } from "../components/ui";

/**
 * Shared image loading for the tools that start from an existing picture.
 *
 * Files are read through the asset protocol and turned into a data URI, which
 * both the canvas and an `<img>` can use directly, and whose base64 half is
 * exactly what the bridge wants.
 */
export function useImageSource() {
  const [source, setSource] = useState<string | null>(null);
  const [data, setData] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function loadFrom(path: string) {
    setError(null);
    try {
      const response = await fetch(convertFileSrc(path));
      const blob = await response.blob();

      await new Promise<void>((resolve) => {
        const reader = new FileReader();
        reader.onload = () => {
          const uri = String(reader.result);
          setSource(uri);
          setData(uri.split(",")[1] ?? null);
          resolve();
        };
        reader.readAsDataURL(blob);
      });
    } catch (err) {
      setError(`Could not open that image: ${errorMessage(err)}`);
    }
  }

  async function pickFile() {
    setBusy(true);
    try {
      const chosen = await open({
        title: "Choose an image",
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
      });
      if (typeof chosen === "string") await loadFrom(chosen);
    } finally {
      setBusy(false);
    }
  }

  function clear() {
    setSource(null);
    setData(null);
  }

  return { source, data, error, busy, setError, loadFrom, pickFile, clear };
}

/** Strip of recent outputs, for starting from something already generated. */
export function RecentStrip({
  images,
  onPick,
}: {
  images: GalleryImage[];
  onPick: (path: string) => void;
}) {
  if (images.length === 0) return null;

  return (
    <div className="studio-footer" style={{ gap: 12, overflowX: "auto" }}>
      <Chip>
        <Images size={12} />
        Recent
      </Chip>
      {images.slice(0, 12).map((image) => (
        <button
          key={image.path}
          className="thumb"
          style={{ width: 58, height: 58, flexShrink: 0 }}
          onClick={() => onPick(image.path)}
          title={image.name}
        >
          <img src={convertFileSrc(image.path)} alt="" loading="lazy" />
        </button>
      ))}
    </div>
  );
}
