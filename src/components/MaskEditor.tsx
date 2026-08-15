import { useCallback, useEffect, useRef, useState } from "react";
import {
  Brush,
  Eraser,
  Maximize2,
  Redo2,
  Trash2,
  Undo2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";

/** Zoom limits. 20x is enough to work on individual pixels of a 1024px image. */
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 20;
const ZOOM_STEP = 1.15;

/** How many mask states we keep for undo. */
const HISTORY_LIMIT = 40;

export interface MaskEditorHandle {
  /** Mask as a base64 PNG: white where the user painted, black elsewhere. */
  getMask: () => string | null;
}

interface Props {
  /** Object URL or data URI of the image being masked. */
  src: string;
  onReady?: (handle: MaskEditorHandle) => void;
}

/**
 * Canvas mask editor with free zoom and pan.
 *
 * Two canvases are stacked: the image, and a mask drawn at the image's native
 * resolution. Both are transformed by the same zoom/pan, so a stroke lands on
 * the same pixel regardless of how far in the view is zoomed — which is the
 * whole point of zooming to mask a small detail accurately.
 */
export function MaskEditor({ src, onReady }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const imageCanvas = useRef<HTMLCanvasElement>(null);
  const maskCanvas = useRef<HTMLCanvasElement>(null);

  // Kept in refs, not state: these change on every pointer move and must not
  // trigger a React render per frame.
  const pan = useRef({ x: 0, y: 0 });
  const zoom = useRef(1);
  const painting = useRef(false);
  const panning = useRef(false);
  const lastPoint = useRef<{ x: number; y: number } | null>(null);

  const history = useRef<ImageData[]>([]);
  const future = useRef<ImageData[]>([]);

  // Brush preview ring. Kept in refs and written straight to the DOM so it can
  // follow the pointer without a render per frame.
  const ring = useRef<HTMLDivElement>(null);
  const pointer = useRef<{ x: number; y: number } | null>(null);
  const brushRef = useRef(48);
  const hideRingTimer = useRef<number | null>(null);

  const [size, setSize] = useState({ width: 0, height: 0 });
  const [brush, setBrush] = useState(48);
  const [erasing, setErasing] = useState(false);
  const [zoomLabel, setZoomLabel] = useState(100);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);

  /**
   * Size and place the brush ring.
   *
   * The brush is defined in image pixels, so its on-screen size is
   * `brush * zoom` — that way the ring always shows the area that will actually
   * be painted, at whatever zoom level the view happens to be at.
   */
  const updateRing = useCallback(() => {
    const element = ring.current;
    const container = containerRef.current;
    if (!element || !container) return;

    const diameter = brushRef.current * zoom.current;
    const position = pointer.current ?? {
      x: container.clientWidth / 2,
      y: container.clientHeight / 2,
    };

    element.style.width = `${diameter}px`;
    element.style.height = `${diameter}px`;
    element.style.left = `${position.x - diameter / 2}px`;
    element.style.top = `${position.y - diameter / 2}px`;
  }, []);

  /** Show the ring, optionally hiding it again after a moment. */
  const showRing = useCallback(
    (autoHideMs?: number) => {
      if (ring.current) ring.current.style.opacity = "1";
      updateRing();

      if (hideRingTimer.current) window.clearTimeout(hideRingTimer.current);
      if (autoHideMs) {
        hideRingTimer.current = window.setTimeout(() => {
          // Keep it visible if the pointer is actually over the canvas.
          if (!pointer.current && ring.current) ring.current.style.opacity = "0";
        }, autoHideMs);
      }
    },
    [updateRing],
  );

  /** Push the current transform onto both canvases. */
  const applyTransform = useCallback(() => {
    const transform = `translate(${pan.current.x}px, ${pan.current.y}px) scale(${zoom.current})`;
    for (const canvas of [imageCanvas.current, maskCanvas.current]) {
      if (canvas) canvas.style.transform = transform;
    }
    setZoomLabel(Math.round(zoom.current * 100));
    updateRing();
  }, [updateRing]);

  // Changing the slider previews the new size straight away, centred in the
  // view when the pointer is elsewhere — no need to paint to find out.
  useEffect(() => {
    brushRef.current = brush;
    showRing(1400);
  }, [brush, showRing]);

  /** Scale the image to fit and centre it. */
  const fitToView = useCallback(() => {
    const container = containerRef.current;
    if (!container || !size.width) return;

    const scale = Math.min(
      (container.clientWidth - 48) / size.width,
      (container.clientHeight - 48) / size.height,
      1,
    );
    zoom.current = Math.max(scale, MIN_ZOOM);
    pan.current = {
      x: (container.clientWidth - size.width * zoom.current) / 2,
      y: (container.clientHeight - size.height * zoom.current) / 2,
    };
    applyTransform();
  }, [size, applyTransform]);

  // Load the image and size both canvases to its native resolution.
  useEffect(() => {
    const image = new Image();
    image.onload = () => {
      const { naturalWidth: width, naturalHeight: height } = image;
      setSize({ width, height });

      for (const canvas of [imageCanvas.current, maskCanvas.current]) {
        if (!canvas) continue;
        canvas.width = width;
        canvas.height = height;
        canvas.style.width = `${width}px`;
        canvas.style.height = `${height}px`;
      }

      imageCanvas.current?.getContext("2d")?.drawImage(image, 0, 0);
      history.current = [];
      future.current = [];
      setCanUndo(false);
      setCanRedo(false);
    };
    image.src = src;
  }, [src]);

  useEffect(() => {
    if (size.width) fitToView();
  }, [size, fitToView]);

  useEffect(() => {
    onReady?.({
      getMask: () => {
        const canvas = maskCanvas.current;
        if (!canvas) return null;

        // Composite onto black: Fooocus treats any non-zero pixel as masked,
        // so the alpha channel alone would not survive the PNG round trip.
        const flat = document.createElement("canvas");
        flat.width = canvas.width;
        flat.height = canvas.height;

        const context = flat.getContext("2d");
        if (!context) return null;
        context.fillStyle = "#000";
        context.fillRect(0, 0, flat.width, flat.height);
        context.drawImage(canvas, 0, 0);

        return flat.toDataURL("image/png").split(",")[1];
      },
    });
  }, [onReady, size]);

  /** Window coordinates -> image pixel coordinates. */
  function toImageSpace(event: React.PointerEvent) {
    const container = containerRef.current;
    if (!container) return { x: 0, y: 0 };

    const rect = container.getBoundingClientRect();
    return {
      x: (event.clientX - rect.left - pan.current.x) / zoom.current,
      y: (event.clientY - rect.top - pan.current.y) / zoom.current,
    };
  }

  function snapshot() {
    const context = maskCanvas.current?.getContext("2d");
    if (!context || !maskCanvas.current) return;

    history.current.push(
      context.getImageData(0, 0, maskCanvas.current.width, maskCanvas.current.height),
    );
    if (history.current.length > HISTORY_LIMIT) history.current.shift();

    future.current = [];
    setCanUndo(true);
    setCanRedo(false);
  }

  function strokeTo(point: { x: number; y: number }) {
    const context = maskCanvas.current?.getContext("2d");
    if (!context) return;

    context.globalCompositeOperation = erasing ? "destination-out" : "source-over";
    context.strokeStyle = "#ffffff";
    context.fillStyle = "#ffffff";
    context.lineCap = "round";
    context.lineJoin = "round";
    context.lineWidth = brush;

    const from = lastPoint.current ?? point;
    context.beginPath();
    context.moveTo(from.x, from.y);
    context.lineTo(point.x, point.y);
    context.stroke();

    lastPoint.current = point;
  }

  function onPointerDown(event: React.PointerEvent) {
    event.currentTarget.setPointerCapture(event.pointerId);

    // Middle mouse, space, or right button pans — leaving left button free to
    // paint at any zoom level.
    if (event.button === 1 || event.button === 2 || event.shiftKey) {
      panning.current = true;
      lastPoint.current = { x: event.clientX, y: event.clientY };
      return;
    }

    if (event.button !== 0) return;
    snapshot();
    painting.current = true;
    lastPoint.current = null;
    strokeTo(toImageSpace(event));
  }

  function onPointerMove(event: React.PointerEvent) {
    const container = containerRef.current;
    if (container) {
      const rect = container.getBoundingClientRect();
      pointer.current = { x: event.clientX - rect.left, y: event.clientY - rect.top };
      updateRing();
    }

    if (panning.current && lastPoint.current) {
      pan.current = {
        x: pan.current.x + (event.clientX - lastPoint.current.x),
        y: pan.current.y + (event.clientY - lastPoint.current.y),
      };
      lastPoint.current = { x: event.clientX, y: event.clientY };
      applyTransform();
      return;
    }

    if (painting.current) strokeTo(toImageSpace(event));
  }

  function onPointerUp() {
    painting.current = false;
    panning.current = false;
    lastPoint.current = null;
  }

  /** Zoom about the cursor, so the pixel under the pointer stays put. */
  function onWheel(event: React.WheelEvent) {
    const container = containerRef.current;
    if (!container) return;

    const rect = container.getBoundingClientRect();
    const cursor = { x: event.clientX - rect.left, y: event.clientY - rect.top };

    const next = Math.min(
      MAX_ZOOM,
      Math.max(MIN_ZOOM, zoom.current * (event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP)),
    );
    const ratio = next / zoom.current;

    pan.current = {
      x: cursor.x - (cursor.x - pan.current.x) * ratio,
      y: cursor.y - (cursor.y - pan.current.y) * ratio,
    };
    zoom.current = next;
    applyTransform();
  }

  function zoomBy(factor: number) {
    const container = containerRef.current;
    if (!container) return;

    const centre = { x: container.clientWidth / 2, y: container.clientHeight / 2 };
    const next = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom.current * factor));
    const ratio = next / zoom.current;

    pan.current = {
      x: centre.x - (centre.x - pan.current.x) * ratio,
      y: centre.y - (centre.y - pan.current.y) * ratio,
    };
    zoom.current = next;
    applyTransform();
  }

  function restore(from: ImageData[], to: ImageData[]) {
    const canvas = maskCanvas.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    const state = from.pop();
    if (!state) return;

    to.push(context.getImageData(0, 0, canvas.width, canvas.height));
    context.putImageData(state, 0, 0);

    setCanUndo(history.current.length > 0);
    setCanRedo(future.current.length > 0);
  }

  function clearMask() {
    const canvas = maskCanvas.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    snapshot();
    context.clearRect(0, 0, canvas.width, canvas.height);
  }

  return (
    <div className="mask-editor">
      <div className="mask-toolbar">
        <div className="mask-tool-group">
          <button
            className={`btn btn-sm${erasing ? "" : " btn-primary"}`}
            onClick={() => setErasing(false)}
            title="Paint the area to change"
          >
            <Brush size={14} />
            Paint
          </button>
          <button
            className={`btn btn-sm${erasing ? " btn-primary" : ""}`}
            onClick={() => setErasing(true)}
            title="Erase part of the mask"
          >
            <Eraser size={14} />
            Erase
          </button>
        </div>

        <label className="mask-brush">
          <span className="field-hint">Brush {brush}px</span>
          <input
            type="range"
            min={2}
            max={300}
            value={brush}
            onChange={(event) => setBrush(Number(event.target.value))}
          />
        </label>

        <div className="mask-tool-group">
          <button
            className="btn btn-sm btn-icon"
            onClick={() => restore(history.current, future.current)}
            disabled={!canUndo}
            title="Undo"
          >
            <Undo2 size={14} />
          </button>
          <button
            className="btn btn-sm btn-icon"
            onClick={() => restore(future.current, history.current)}
            disabled={!canRedo}
            title="Redo"
          >
            <Redo2 size={14} />
          </button>
          <button className="btn btn-sm btn-icon" onClick={clearMask} title="Clear mask">
            <Trash2 size={14} />
          </button>
        </div>

        <div className="mask-tool-group" style={{ marginLeft: "auto" }}>
          <button
            className="btn btn-sm btn-icon"
            onClick={() => zoomBy(1 / ZOOM_STEP)}
            title="Zoom out"
          >
            <ZoomOut size={14} />
          </button>
          <span className="mask-zoom">{zoomLabel}%</span>
          <button
            className="btn btn-sm btn-icon"
            onClick={() => zoomBy(ZOOM_STEP)}
            title="Zoom in"
          >
            <ZoomIn size={14} />
          </button>
          <button className="btn btn-sm btn-icon" onClick={fitToView} title="Fit to view">
            <Maximize2 size={14} />
          </button>
        </div>
      </div>

      <div
        className="mask-viewport"
        ref={containerRef}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerEnter={() => showRing()}
        onPointerLeave={(event) => {
          onPointerUp();
          pointer.current = null;
          if (ring.current) ring.current.style.opacity = "0";
          event.currentTarget.releasePointerCapture?.(event.pointerId);
        }}
        onWheel={onWheel}
        onContextMenu={(event) => event.preventDefault()}
      >
        <canvas ref={imageCanvas} className="mask-layer" />
        <canvas ref={maskCanvas} className="mask-layer mask-overlay" />
        <div
          ref={ring}
          className={`brush-ring${erasing ? " erasing" : ""}`}
          aria-hidden="true"
        />
      </div>

      <p className="mask-help field-hint">
        Drag to paint the area you want changed. Scroll to zoom around the cursor, and hold shift
        or the middle mouse button to pan.
      </p>
    </div>
  );
}
