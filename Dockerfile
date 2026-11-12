# ----------------------------------------------------------------------
# Stage 1: Build Rust binary
# NOTE: pinned to bookworm so the built binary links against glibc 2.36,
# matching the runtime image below (rust:1-slim now defaults to Debian
# trixie / glibc 2.39+, which crashes on a bookworm runtime with
# "weak version GLIBC_2.39 not found").
# ----------------------------------------------------------------------
FROM rust:1-slim-bookworm AS builder

WORKDIR /app

# Copy cargo manifests
COPY Cargo.toml Cargo.lock* ./

# Copy source code
COPY src ./src

# Build release binary (using -j 2 to keep build memory within cloud runner limits)
RUN cargo build --release -j 2

# ----------------------------------------------------------------------
# Stage 2: Minimal runtime container
# ----------------------------------------------------------------------
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies: FFmpeg, Samsung rlottie, ca-certificates for Telegram HTTPS
RUN apt-get update && apt-get install -y --no-install-recommends \
    ffmpeg \
    librlottie0-1 \
    librlottie-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy compiled Rust executable from builder
COPY --from=builder /app/target/release/sticker_converter_bot /app/sticker_converter_bot

# Copy the .env file (contains BOT_TOKEN) into the image.
# For better security, delete this line and pass the token at runtime instead:
#   docker run -e BOT_TOKEN="..." sticker-converter-bot
# Runtime environment variables still take precedence over .env values.
COPY .env /app/.env

# Create temporary directory for conversions
RUN mkdir -p /app/temp

ENV TEMP_DIR=/app/temp
ENV FFMPEG_PATH=ffmpeg
# Debian's rlottie package names the runtime file with a trailing "-1"
# (SONAME quirk): the real file is librlottie.so.0-1, NOT librlottie.so.0.
ENV RLOTTIE_DLL_PATH=/usr/lib/x86_64-linux-gnu/librlottie.so.0-1

# Safety net: if the configured rlottie path doesn't exist in this base
# image, discover it at container start before launching the bot.
RUN printf '#!/bin/sh\nif [ ! -f "$RLOTTIE_DLL_PATH" ]; then\n  FOUND=$(find /usr/lib -name "librlottie.so*" 2>/dev/null | head -n 1)\n  [ -n "$FOUND" ] && export RLOTTIE_DLL_PATH="$FOUND"\nfi\nexec "$@"\n' > /app/entrypoint.sh && chmod +x /app/entrypoint.sh

ENTRYPOINT ["/app/entrypoint.sh"]
CMD ["/app/sticker_converter_bot"]
