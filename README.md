<img src="brand/logo.png" width="104" alt="">

# Fooocus-Front

A desktop app that makes [Fooocus](https://github.com/lllyasviel/Fooocus) easy to install, run and
use — no terminal, no console windows, no setup guides.

![status](https://img.shields.io/badge/status-in%20development-orange)
![platform](https://img.shields.io/badge/platform-Windows-blue)
![built%20with](https://img.shields.io/badge/built%20with-Tauri%202%20%2B%20React-5b5bd6)

## Why this exists

Fooocus is genuinely excellent, and it is free. But getting it running puts a lot of people off
before they ever generate an image:

- You have to find the right download among release tags, and the URL most guides link to now
  serves a build from 2023.
- You unzip it and run a `.bat` file, which leaves a black console window open the whole time.
- If you have an **AMD** card you must hand-edit that `.bat` file to swap out PyTorch.
- If you have an **Intel Arc** card there are *no official instructions at all* — you are on your
  own working out which Intel packages to install.
- The first time you press Generate it silently downloads about 7 GB with no progress shown, so it
  looks frozen.
- Downloading a model means finding it yourself and dropping the file in the correct folder.

None of that is hard if you're technical. All of it is a wall if you're not — and frankly it's
tedious even if you are. This app does all of it for you.

Pick your graphics card, press install, and it handles the rest.

## What it does

### Sets Fooocus up from scratch

If no Fooocus installation is found, the app offers to install one. It resolves the official
package from the GitHub API, downloads it with real progress (resuming if interrupted), extracts
it, and deletes the archive afterwards.

Then it configures the correct PyTorch stack **for your specific graphics card**:

| Graphics card | What happens |
| --- | --- |
| NVIDIA | Nothing extra needed — the standard package already suits it |
| Intel Arc | Installs Intel's XPU packages (`intel-extension-for-pytorch`). **Not documented by Fooocus** |
| AMD | Installs `torch-directml`, following the official Fooocus instructions |
| CPU only | Runs on the processor, with an honest warning about speed |

Your card is detected automatically and you can override the choice.

Finally it offers to fetch the models Fooocus needs — about 7 GB — with a real progress bar. Skip
that and Fooocus downloads them silently the first time you press Generate, which looks like the
app has frozen for a very long time.

**No git, no Python, no pip, no command line.** The package ships its own interpreter.

### Runs Fooocus invisibly

Fooocus runs hidden in the background. There is no console window at any point, and closing the app
shuts it down cleanly rather than leaving several gigabytes of video memory occupied.

Startup progress appears in the app — "Detecting hardware", "Loading base model", "Loading VAE" —
instead of scrolling console text.

### Studio — a native interface

Generation is driven directly, not through the embedded web UI:

- **Generate** — prompt, aspect ratio, performance mode, image count, presets, seed
- **Inpaint & Outpaint** — a proper mask editor with **free zoom and pan**, a brush preview that
  scales with zoom, undo/redo, and Fooocus's three inpaint methods
- **Upscale & Vary** — all five methods, with plain-language explanations
- **Image Prompt** — up to four reference images across ImagePrompt, PyraCanny, CPDS and FaceSwap,
  each with weight and stop-at controls
- **Fooocus UI** — the original interface, still available for anything not yet ported

Live step-by-step previews as the image forms, with progress visible from every screen.

### Manages models properly

Fooocus normally leaves you to find model files yourself and drop them in the right folder, and it
downloads what it needs silently mid-generation.

- **See what you have**, by category, with sizes and a shortcut to reveal any file on disk.
- **A curated catalog** built from your own installation's `modules/config.py` and `presets/*.json`,
  so the download links always match your version rather than a hardcoded guess. Two files are
  deliberately renamed on the way down, because Fooocus expects names that differ from the URL.
- **Warnings before you generate**, when a feature needs a model you do not have — rather than
  discovering it as a stall halfway through.

### Browse Civitai without the mess

Civitai has an enormous library and a hard-to-navigate site. The browser is built in:

- **Only shows what Fooocus can run.** Fooocus is SDXL-only, and a large share of Civitai is
  SD 1.5 — which downloads happily and then fails to load. SDXL-family models are the default
  filter; turn it off and incompatible versions are labelled and cannot be downloaded by mistake.
- **Files land in the right folder** automatically, chosen from the model's type.
- **Already-installed models are marked**, so you are not offered something you own.
- **Content controls** mirroring Civitai's own: one-click toggles for anime, furry, gore and
  political, plus your own hidden-tag list. NSFW is hidden by default.
- **Version picker**, since popular checkpoints have a dozen or more releases.

Browsing needs no account. Downloading requires a free Civitai API key, which they enforce; it is
verified before being saved and stored in **Windows Credential Manager**, not in a file.

### Downloads that behave

- Real progress, speed and estimated time, with pause, resume and cancel.
- **The queue survives closing the app.** Anything downloading resumes from its partial file;
  anything paused stays paused.
- **A selectable limit** on how many run at once (default 2). Queuing a dozen models at once
  finishes them all later, not sooner.
- Files download as `.part` and are only renamed into place when complete, so an interrupted
  transfer can never be mistaken for an installed model.

### Everything else

A gallery of your outputs grouped by day, and a settings screen for install location, startup
behaviour and Fooocus's own paths.

## Installation

Download the installer from [Releases](../../releases), run it, and open the app.

Windows will warn that the app is unsigned — click **More info → Run anyway**. Signing costs a few
hundred pounds a year, which is hard to justify for a free tool.

You'll need roughly **15 GB free**: 2 GB for the package, ~4 GB extracted, ~7 GB for models.

## A deliberate promise

**Your Fooocus installation is never modified.** The app reads your `.bat` files rather than
running them, reads `config.txt` for your model paths, and keeps its own files — settings, the
download queue, the bridge script — in its own data directory. Models you download go into the
folders `config.txt` already points at.

Two deliberate exceptions, both of which you ask for explicitly: the graphics setup replaces
PyTorch inside the Fooocus folder, because that is the whole point of it; and the bridge is loaded
into the Fooocus process at launch, though it lives outside the installation.

## How it works

Fooocus has no official API. Rather than scrape its web interface, this app runs a small bridge
**inside** the Fooocus process. It puts jobs onto Fooocus's own internal queue and reads back
progress, live previews and results — while the normal Fooocus interface still starts up and
remains available.

Two Windows-specific details that make the invisible launch work:

1. Every Fooocus `.bat` ends in `pause`. Run hidden, that blocks forever on a keypress that can
   never arrive — so the app reads the `.bat`, takes the Python arguments out of it, and runs the
   interpreter itself.
2. Python block-buffers its output when writing to a pipe rather than a console, so a quiet startup
   produces no output for minutes and looks like a hang. The app runs Python unbuffered.

## Building from source

Requirements: [Rust](https://rustup.rs/), Node.js 20+, and Visual Studio Build Tools with the
*Desktop development with C++* workload.

```bash
npm install
npm run tauri dev      # development, with hot reload
npm run tauri build    # produces the installer
```

Output lands in `src-tauri/target/release/bundle/nsis/`.

## Project layout

```
src/                    React frontend
  screens/              One file per screen and Studio mode
  components/           Mask editor, sidebar, shared UI
  lib/api.ts            Typed wrappers over every backend command
  store.ts              Application state and event wiring

src-tauri/src/
  installer.rs          From-scratch install, GPU detection and configuration
  launcher.rs           Hidden process launch, log streaming, startup progress
  bridge.rs             Client for the in-process Python bridge
  catalog.rs            Model catalog derived from the Fooocus source
  civitai.rs            Civitai search, filtering and installed detection
  downloads.rs          Download queue: scheduling, resume, persistence
  secrets.rs            API keys, stored in the OS credential store
  install.rs            Install discovery and config parsing

src-tauri/resources/
  fooocus_bridge.py     Runs inside Fooocus; queue access and event streaming

brand/                  Logo master, SVG, and the script that generates it
```

## Status

Working and in daily use, but young. Known gaps:

- Styles and LoRAs are not yet in the native Studio (both available via the Fooocus UI tab)
- No interface translation, and no prompt translation for non-English users. Worth knowing that
  SDXL itself understands English far better than anything else, so translated buttons alone would
  not make it equally usable
- The from-scratch installer has been built but not yet run end-to-end on a clean machine
- NVIDIA and AMD graphics setup follow the documented steps but are untested. Only Intel Arc has
  been verified, on the machine this was developed on
- Windows only. Fooocus supports Mac via MPS, but this app assumes `.bat` files, PowerShell and the
  Windows standalone package

## Licence

[GPL-3.0](LICENSE), matching Fooocus itself.

## Credits

All the actual image generation is [Fooocus](https://github.com/lllyasviel/Fooocus) by
[lllyasviel](https://github.com/lllyasviel) and its contributors. This project is an independent
front end and is not affiliated with the Fooocus project.
