"""
Fooocus Frontend bridge.

Runs *inside* the Fooocus process. It starts a small loopback HTTP server in a
background thread and then hands control to Fooocus's own `launch.py`, so the
normal Gradio interface still comes up exactly as it would have. Both share the
same `modules.async_worker.async_tasks` queue, which is what lets a native UI
drive generation while the original interface remains available.

Nothing in the Fooocus installation is modified. This file lives in the app's
own data directory and is passed to the embedded interpreter by path.

Generation arguments are a flat positional list, in the order
`AsyncTask.__init__` pops them. Rather than hardcode 80+ values, we build the
full list from the installation's own `modules.config` defaults and overwrite
only the fields the UI actually exposes. New Fooocus options therefore inherit
that install's defaults instead of breaking the call.
"""

import base64
import io
import json
import os
import runpy
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

# Populated once Fooocus has finished importing.
_ready = threading.Event()
_lock = threading.Lock()

# Append-only event log. The UI polls with `?since=` and gets everything after
# that index, so a slow or briefly disconnected client never misses anything.
_events = []
_jobs = {}
_token = ""


# --------------------------------------------------------------------- events


def _emit(kind, **payload):
    with _lock:
        payload["kind"] = kind
        payload["index"] = len(_events)
        payload["time"] = time.time()
        _events.append(payload)


def _events_since(since):
    with _lock:
        return _events[since:] if since < len(_events) else []


# ------------------------------------------------------------------ arguments


def _encode_image(image):
    """Numpy array or PIL image -> base64 PNG for transport over JSON."""
    try:
        from PIL import Image
        import numpy as np

        if image is None:
            return None
        if isinstance(image, np.ndarray):
            image = Image.fromarray(image.astype("uint8"))

        buffer = io.BytesIO()
        image.save(buffer, format="PNG")
        return base64.b64encode(buffer.getvalue()).decode("ascii")
    except Exception:
        # A preview that cannot be encoded must never take the job down.
        return None


def _decode_image(data, mode="RGB"):
    """base64 PNG -> HxWx3 uint8 numpy array, as Fooocus expects."""
    if not data:
        return None

    from PIL import Image
    import numpy as np

    if "," in data[:64]:  # tolerate a data: URI prefix
        data = data.split(",", 1)[1]

    image = Image.open(io.BytesIO(base64.b64decode(data))).convert(mode)
    return np.array(image).astype("uint8")


def _inpaint_payload(overrides):
    """Build the {'image', 'mask'} dict for an inpaint job, or None.

    Fooocus reads the mask as `mask[:, :, 0]`, so both arrays must be 3-channel
    and the same size. Anything non-zero in the mask is repainted.
    """
    import numpy as np

    image = _decode_image(overrides.get("input_image"))
    if image is None:
        return None

    mask = _decode_image(overrides.get("mask_image"))
    if mask is None:
        mask = np.zeros_like(image)
    elif mask.shape[:2] != image.shape[:2]:
        from PIL import Image

        resized = Image.fromarray(mask).resize(
            (image.shape[1], image.shape[0]), Image.NEAREST
        )
        mask = np.array(resized).astype("uint8")

    return {"image": image, "mask": mask}


def _build_args(overrides):
    """Assemble the positional argument list for AsyncTask.

    Order mirrors `AsyncTask.__init__` exactly. Every value defaults to this
    installation's configured default; `overrides` replaces only what the UI
    sent.
    """
    import modules.config as config
    import args_manager

    def value(name, fallback=None):
        return overrides.get(name, getattr(config, "default_" + name, fallback))

    args = []

    args.append(False)                                   # generate_image_grid
    args.append(value("prompt", ""))
    args.append(overrides.get("negative_prompt", getattr(config, "default_prompt_negative", "")))
    args.append(value("styles", []))
    args.append(value("performance"))
    args.append(value("aspect_ratio"))
    args.append(int(value("image_number", 1)))
    args.append(value("output_format", "png"))
    args.append(int(overrides.get("seed", 0)))
    args.append(False)                                   # read_wildcards_in_order
    args.append(overrides.get("sharpness", getattr(config, "default_sample_sharpness", 2.0)))
    args.append(overrides.get("cfg_scale", getattr(config, "default_cfg_scale", 7.0)))
    args.append(value("base_model_name"))
    args.append(value("refiner_model_name"))
    args.append(value("refiner_switch"))

    # LoRAs: a fixed number of (enabled, name, weight) triples.
    loras = overrides.get("loras", getattr(config, "default_loras", []))
    for index in range(config.default_max_lora_number):
        if index < len(loras):
            entry = loras[index]
            # Config stores either (enabled, name, weight) or (name, weight).
            if len(entry) == 3:
                enabled, name, weight = entry
            else:
                enabled, (name, weight) = True, entry
            args.extend([bool(enabled), str(name), float(weight)])
        else:
            args.extend([False, "None", 1.0])

    # Image input. `current_tab` picks which tool consumes the image:
    # 'inpaint' wants an {image, mask} dict, 'uov' wants a bare array.
    tab = overrides.get("current_tab", "uov")
    inpaint = _inpaint_payload(overrides) if tab == "inpaint" else None
    uov_image = _decode_image(overrides.get("input_image")) if tab == "uov" else None
    has_ip = tab == "ip" and any(
        slot.get("image") for slot in (overrides.get("ip_slots") or []))
    uov_method = overrides.get(
        "uov_method", getattr(config, "default_uov_method", "Disabled"))

    args.append(                                         # input_image_checkbox
        inpaint is not None or uov_image is not None or has_ip)
    args.append(tab)
    args.append(uov_method)
    args.append(uov_image)
    args.append(overrides.get("outpaint_selections", []))
    args.append(inpaint)
    args.append(overrides.get("inpaint_additional_prompt", ""))
    args.append(None)                                    # inpaint_mask_image_upload

    args.append(overrides.get("disable_preview", False))
    args.append(False)                                   # disable_intermediate_results
    args.append(overrides.get("disable_seed_increment", False))
    args.append(getattr(config, "default_black_out_nsfw", False))

    args.append(1.5)                                     # adm_scaler_positive
    args.append(0.8)                                     # adm_scaler_negative
    args.append(0.3)                                     # adm_scaler_end
    args.append(getattr(config, "default_cfg_tsnr", 7.0))
    args.append(getattr(config, "default_clip_skip", 2))
    args.append(getattr(config, "default_sampler", "dpmpp_2m_sde_gpu"))
    args.append(getattr(config, "default_scheduler", "karras"))
    args.append(getattr(config, "default_vae", "Default (model)"))

    args.append(getattr(config, "default_overwrite_step", -1))
    args.append(getattr(config, "default_overwrite_switch", -1))
    args.append(-1)                                      # overwrite_width
    args.append(-1)                                      # overwrite_height
    args.append(-1)                                      # overwrite_vary_strength
    args.append(getattr(config, "default_overwrite_upscale", -1))

    args.append(False)                                   # mixing_image_prompt_and_vary_upscale
    args.append(False)                                   # mixing_image_prompt_and_inpaint
    args.append(False)                                   # debugging_cn_preprocessor
    args.append(False)                                   # skipping_cn_preprocessor
    args.append(64)                                      # canny_low_threshold
    args.append(128)                                     # canny_high_threshold
    args.append("joint")                                 # refiner_swap_method
    args.append(0.25)                                    # controlnet_softness

    args.extend([False, 1.01, 1.02, 0.99, 0.95])         # freeu

    args.append(False)                                   # debugging_inpaint_preprocessor
    args.append(overrides.get("inpaint_disable_initial_latent", False))
    args.append(overrides.get(
        "inpaint_engine", getattr(config, "default_inpaint_engine_version", "v2.6")))
    args.append(float(overrides.get("inpaint_strength", 1.0)))
    args.append(float(overrides.get("inpaint_respective_field", 0.618)))
    args.append(getattr(config, "default_inpaint_advanced_masking_checkbox", False))
    args.append(getattr(config, "default_invert_mask_checkbox", False))
    args.append(0)                                       # inpaint_erode_or_dilate

    # These three are conditional on CLI flags, exactly as AsyncTask pops them.
    if not args_manager.args.disable_image_log:
        args.append(getattr(config, "default_save_only_final_enhanced_image", False))
    if not args_manager.args.disable_metadata:
        args.append(getattr(config, "default_save_metadata_to_images", False))
        args.append(getattr(config, "default_metadata_scheme", "fooocus"))

    # Image prompt slots. Any slot the UI supplied gets its image and settings;
    # the rest stay empty. These config maps are keyed 1..n rather than being
    # lists, so the loop is 1-indexed.
    import modules.flags as flags

    slots = overrides.get("ip_slots") or []
    for index in range(config.default_controlnet_image_count):
        slot = index + 1
        supplied = slots[index] if index < len(slots) else None

        if supplied and supplied.get("image"):
            cn_type = supplied.get("type", flags.cn_ip)
            fallback_stop, fallback_weight = flags.default_parameters.get(
                cn_type, (0.5, 0.6))
            args.extend([
                _decode_image(supplied["image"]),
                float(supplied.get("stop", fallback_stop)),
                float(supplied.get("weight", fallback_weight)),
                cn_type,
            ])
        else:
            args.extend([
                None,
                config.default_ip_stop_ats.get(slot, 0.5),
                config.default_ip_weights.get(slot, 0.6),
                config.default_ip_types.get(slot, flags.cn_ip),
            ])

    args.append(False)                                   # debugging_dino
    args.append(0)                                       # dino_erode_or_dilate
    args.append(False)                                   # debugging_enhance_masks_checkbox

    args.append(None)                                    # enhance_input_image
    args.append(False)                                   # enhance_checkbox
    args.append(getattr(config, "default_enhance_uov_method", "Disabled"))
    args.append(getattr(config, "default_enhance_uov_processing_order", "Before First Enhancement"))
    args.append(getattr(config, "default_enhance_uov_prompt_type", "Original Prompts"))

    for _ in range(config.default_enhance_tabs):
        args.extend([
            False,                                       # enhance_enabled
            "",                                          # mask_dino_prompt_text
            "",                                          # prompt
            "",                                          # negative_prompt
            getattr(config, "default_enhance_inpaint_mask_model", "sam"),
            getattr(config, "default_inpaint_mask_cloth_category", "full"),
            getattr(config, "default_inpaint_mask_sam_model", "vit_b"),
            0.25,                                        # text_threshold
            0.3,                                         # box_threshold
            getattr(config, "default_sam_max_detections", 0),
            False,                                       # inpaint_disable_initial_latent
            getattr(config, "default_inpaint_engine_version", "v2.6"),
            1.0,                                         # inpaint_strength
            0.618,                                       # inpaint_respective_field
            0,                                           # inpaint_erode_or_dilate
            False,                                       # mask_invert
        ])

    return args


# ----------------------------------------------------------------- generation


def _watch(task, job_id):
    """Drain a task's yields into our event log until it finishes.

    A freshly queued task has `processing == False` until the worker picks it
    up, so "not processing" only means "finished" once we have seen it start.
    The worker pops the task off `async_tasks` when it begins, which is the
    signal we watch for.
    """
    from modules.async_worker import async_tasks

    seen = 0
    started = False

    while True:
        while seen < len(task.yields):
            kind, payload = task.yields[seen]
            seen += 1

            if kind == "preview":
                percentage, title, image = payload
                _emit(
                    "preview",
                    jobId=job_id,
                    percentage=percentage,
                    title=title,
                    image=_encode_image(image),
                )
            elif kind == "results":
                _emit("results", jobId=job_id, count=len(payload))
            elif kind == "finish":
                paths = [p for p in task.results if isinstance(p, str)]
                _emit("finish", jobId=job_id, images=paths)
                _jobs.pop(job_id, None)
                return

        if not started:
            if task.processing or task not in async_tasks:
                started = True
                _emit("started", jobId=job_id)
        elif not task.processing and seen >= len(task.yields):
            # Ran to completion without a trailing finish yield.
            paths = [p for p in task.results if isinstance(p, str)]
            _emit("finish", jobId=job_id, images=paths)
            _jobs.pop(job_id, None)
            return

        time.sleep(0.1)


def _generate(overrides):
    from modules.async_worker import AsyncTask, async_tasks

    task = AsyncTask(args=_build_args(overrides))
    job_id = "job-%d" % int(time.time() * 1000)
    _jobs[job_id] = task

    async_tasks.append(task)
    _emit("queued", jobId=job_id)

    threading.Thread(target=_watch, args=(task, job_id), daemon=True).start()
    return job_id


def _options():
    """Everything the UI needs to populate its controls."""
    import modules.config as config
    import modules.flags as flags
    import modules.sdxl_styles as sdxl_styles
    import re

    # Ratio labels carry inline HTML for Gradio's benefit. AsyncTask needs the
    # exact original string, so send both: the raw value and a clean label.
    ratios = []
    for raw in getattr(config, "available_aspect_ratios_labels", []):
        text = re.sub(r"<[^>]+>", "", raw).replace("∣", "·")
        ratios.append({"value": raw, "label": " ".join(text.split())})

    return {
        "styles": sorted(getattr(sdxl_styles, "legal_style_names", [])),
        "aspectRatios": ratios,
        "uovMethods": [m for m in getattr(flags, "uov_list", []) if m != flags.disabled],
        "ipTypes": list(getattr(flags, "ip_list", [])),
        # Per-type (stop, weight) starting points Fooocus itself uses.
        "ipDefaults": {
            name: list(values)
            for name, values in getattr(flags, "default_parameters", {}).items()
        },
        "ipSlotCount": config.default_controlnet_image_count,
        "maxLoraNumber": config.default_max_lora_number,
        "loraMinWeight": getattr(config, "default_loras_min_weight", -2),
        "loraMaxWeight": getattr(config, "default_loras_max_weight", 2),
        "performances": [p.value for p in flags.Performance],
        "baseModels": getattr(config, "model_filenames", []),
        "outputFormats": list(getattr(flags, "output_formats", ["png", "jpeg", "webp"])),
        "defaults": {
            "prompt": getattr(config, "default_prompt", ""),
            "negativePrompt": getattr(config, "default_prompt_negative", ""),
            "styles": getattr(config, "default_styles", []),
            "performance": getattr(config, "default_performance", ""),
            "aspectRatio": getattr(config, "default_aspect_ratio", ""),
            "imageNumber": getattr(config, "default_image_number", 1),
            "baseModel": getattr(config, "default_base_model_name", ""),
            "outputFormat": getattr(config, "default_output_format", "png"),
            "sharpness": getattr(config, "default_sample_sharpness", 2.0),
            "cfgScale": getattr(config, "default_cfg_scale", 7.0),
            "maxImageNumber": getattr(config, "default_max_image_number", 32),
        },
    }


# --------------------------------------------------------------------- server


class Handler(BaseHTTPRequestHandler):
    # Silence per-request logging; it would interleave with Fooocus's output.
    def log_message(self, *_args):
        pass

    def _authorised(self):
        return self.headers.get("X-Bridge-Token", "") == _token

    def _reply(self, status, body):
        raw = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def do_GET(self):
        if not self._authorised():
            return self._reply(403, {"error": "forbidden"})

        path, _, query = self.path.partition("?")
        params = dict(
            part.split("=", 1) for part in query.split("&") if "=" in part
        )

        if path == "/health":
            return self._reply(200, {"ready": _ready.is_set()})

        if not _ready.is_set():
            return self._reply(503, {"error": "Fooocus is still starting"})

        if path == "/options":
            try:
                return self._reply(200, _options())
            except Exception as error:
                return self._reply(500, {"error": str(error)})

        if path == "/events":
            since = int(params.get("since", "0"))
            return self._reply(200, {"events": _events_since(since)})

        return self._reply(404, {"error": "not found"})

    def do_POST(self):
        if not self._authorised():
            return self._reply(403, {"error": "forbidden"})
        if not _ready.is_set():
            return self._reply(503, {"error": "Fooocus is still starting"})

        length = int(self.headers.get("Content-Length", "0"))
        try:
            body = json.loads(self.rfile.read(length) or b"{}")
        except Exception:
            return self._reply(400, {"error": "invalid JSON"})

        if self.path == "/generate":
            try:
                return self._reply(200, {"jobId": _generate(body)})
            except Exception as error:
                return self._reply(500, {"error": "%s: %s" % (type(error).__name__, error)})

        if self.path in ("/stop", "/skip"):
            action = "stop" if self.path == "/stop" else "skip"
            for task in list(_jobs.values()):
                task.last_stop = action
            return self._reply(200, {"ok": True})

        return self._reply(404, {"error": "not found"})


def _serve(port):
    """Wait for Fooocus to finish importing, then accept requests."""
    while True:
        try:
            import modules.async_worker  # noqa: F401
            import modules.config  # noqa: F401

            _ready.set()
            break
        except Exception:
            time.sleep(0.5)

    ThreadingHTTPServer(("127.0.0.1", port), Handler).serve_forever()


def main():
    global _token

    argv = sys.argv[1:]
    port = int(argv[argv.index("--bridge-port") + 1])
    _token = argv[argv.index("--bridge-token") + 1]
    launch_py = argv[argv.index("--fooocus-launch") + 1]

    # Everything after `--` belongs to Fooocus.
    passthrough = argv[argv.index("--") + 1:] if "--" in argv else []

    server = threading.Thread(target=_serve, args=(port,), daemon=True)
    server.start()
    print("[Bridge] listening on 127.0.0.1:%d" % port, flush=True)

    # Hand over to Fooocus. `launch.py` derives its own root from __file__ and
    # chdirs there, so running it by absolute path behaves identically to the
    # stock bats. This blocks for the life of the process.
    sys.argv = [launch_py] + passthrough
    runpy.run_path(launch_py, run_name="__main__")


if __name__ == "__main__":
    main()
