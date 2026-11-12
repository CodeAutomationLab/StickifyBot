# StickifyBot | Sticker Download Bot

> 🤖 **StickifyBot – Your All-in-One Telegram Sticker Downloader!**
>
> 📥 Download Telegram stickers instantly.  
> 🔄 Convert stickers into GIF, MP4, PNG, WEBP, JPG and more.  
> 🎬 Supports static, animated and video stickers.  
> 📦 Download complete sticker packs.  
>
> **Fast • Free • Easy to Use**

---

A native, ultra-fast Telegram Bot written in Rust that converts Telegram animated stickers (`.tgs`), video stickers (`.webm`), static stickers (`.webp`), and emojis into MP4 videos, transparent WebM, WEBP, GIF, Lottie JSON, and PNG frames.

---

## ⚡ Performance Highlights

- **Zero-Disk Intermediate Frames:** Raw BGRA frame buffers are streamed directly into FFmpeg `stdin` (`-f rawvideo`). No disk writes for intermediate frames.
- **Sub-110ms Latency:** Converts standard 60-frame animated stickers to MP4 in **~107 ms** (278 FPS throughput).
- **Sub-1ms PNG Frames:** Extracts the first frame as a lossless PNG in **~930 µs**.
- **Ultra-Low Memory:** Consumes only **18–35 MB RAM** (vs 180–320 MB in Python).
- **Exact Native Framerate:** Automatically detects and synchronizes 60 FPS / 30 FPS sticker playback speeds.

---

## 🛠 Features

- **Modern Telegram UI/UX (Bot API 10.3):** Compact inline keyboard layout with interactive submenus for Resolution, Background, and Framerate.
- **Supported Input Formats:**
  - Animated Stickers (`.tgs` gzip Lottie)
  - Video Stickers (`.webm` VP9)
  - Static Stickers (`.webp`)
  - Unicode Emojis (resolved via Telegram's `AnimatedEmojies` sticker set)
  - Telegram Premium Custom Emojis (`get_custom_emoji_stickers`)
- **Supported Output Formats:**
  - 🎬 **MP4 Video** (H.264 Faststart)
  - 🌐 **WebM** (VP9 `yuva420p` with true transparency)
  - 🖼 **WEBP** (Lossless animated WEBP)
  - 🎞 **GIF** (Two-pass high-resolution palettegen)
  - 📦 **Lottie JSON** (Raw uncompressed vector Lottie)
  - 📸 **PNG Frame** (Lossless first frame snapshot)
  - 🖼 **JPG / Static Image** (Fast image conversion)

---

## 🚀 Deployment with Docker

This repository includes a multi-stage `Dockerfile` optimized for Linux container environments:

```bash
docker build -t stickify-bot .
docker run -d --name stickify-bot \
  -e BOT_TOKEN="your_bot_token_here" \
  stickify-bot
```

### Hosting on Voroa.com / Cloud VPS

1. Create a **Background Worker** service on your cloud platform.
2. Connect this repository (`CodeAutomationLab/StickifyBot`).
3. Add the `BOT_TOKEN` environment variable in the service settings.
4. Deploy!

---

## 💻 Local Development

### Requirements
- Rust toolchain (`stable`)
- FFmpeg on PATH
- Samsung RLottie (`bin/rlottie.dll` on Windows, or `librlottie-dev` on Linux)

### Setup & Run
```bash
# Clone the repository
git clone https://github.com/CodeAutomationLab/StickifyBot.git
cd StickifyBot

# Copy example environment configuration
cp .env.example .env

# Run locally in release mode
cargo run --release
```

### Running Tests
```bash
cargo test --release
```

### Running Benchmarks
```bash
cargo run --release --bin benchmark
```

---

## 📄 License

MIT License. Free to use, modify, and distribute.
