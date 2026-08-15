import { useEffect, useMemo, useState } from "react";
import { FolderOpen, Images, RefreshCw, X } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { formatBytes, formatDay } from "../lib/format";
import { useStore } from "../store";
import { EmptyState, ScreenHeader } from "../components/ui";

export function Gallery() {
  const { images, refreshGallery, install } = useStore();
  const [preview, setPreview] = useState<string | null>(null);

  // Refresh on mount so images generated in the Studio show up immediately.
  useEffect(() => {
    void refreshGallery();
  }, [refreshGallery]);

  // Close the lightbox with Escape, as any image viewer should.
  useEffect(() => {
    if (!preview) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPreview(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [preview]);

  // Fooocus already groups output by day via its folder names.
  const days = useMemo(() => {
    const grouped = new Map<string, typeof images>();
    for (const image of images) {
      const bucket = grouped.get(image.day) ?? [];
      bucket.push(image);
      grouped.set(image.day, bucket);
    }
    return [...grouped.entries()].sort((a, b) => b[0].localeCompare(a[0]));
  }, [images]);

  return (
    <div className="screen">
      <ScreenHeader
        title="Gallery"
        subtitle={
          images.length
            ? `${images.length} images from ${install?.outputsDir ?? "your outputs folder"}`
            : "Everything Fooocus has generated"
        }
        actions={
          <>
            {install && (
              <button className="btn" onClick={() => void revealItemInDir(install.outputsDir)}>
                <FolderOpen size={15} />
                Open folder
              </button>
            )}
            <button className="btn" onClick={() => void refreshGallery()}>
              <RefreshCw size={15} />
              Refresh
            </button>
          </>
        }
      />

      <div className="screen-body">
        {images.length === 0 ? (
          <EmptyState icon={<Images size={22} />} title="Nothing generated yet">
            Images you create appear here, newest first, grouped by the day they were made.
          </EmptyState>
        ) : (
          days.map(([day, items]) => (
            <section className="gallery-day" key={day}>
              <h2 className="gallery-day-title">
                {formatDay(day)} · {items.length}
              </h2>
              <div className="gallery-grid">
                {items.map((image) => (
                  <button
                    className="thumb"
                    key={image.path}
                    onClick={() => setPreview(image.path)}
                    title={image.name}
                  >
                    <img src={convertFileSrc(image.path)} alt={image.name} loading="lazy" />
                    <span className="thumb-label truncate">
                      {image.name} · {formatBytes(image.size)}
                    </span>
                  </button>
                ))}
              </div>
            </section>
          ))
        )}
      </div>

      {preview && (
        <div className="lightbox" onClick={() => setPreview(null)}>
          <div className="lightbox-bar" onClick={(event) => event.stopPropagation()}>
            <button className="btn btn-sm" onClick={() => void revealItemInDir(preview)}>
              <FolderOpen size={14} />
              Show in folder
            </button>
            <button className="btn btn-sm" onClick={() => setPreview(null)}>
              <X size={14} />
            </button>
          </div>
          <img
            src={convertFileSrc(preview)}
            alt=""
            onClick={(event) => event.stopPropagation()}
          />
        </div>
      )}
    </div>
  );
}
