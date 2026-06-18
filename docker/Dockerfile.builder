FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    git pkg-config ca-certificates curl wget file \
    libwebkit2gtk-4.1-dev \
    libsoup-3.0-dev \
    libjavascriptcoregtk-4.1-dev \
    libasound2-dev \
    libappindicator3-dev \
    librsvg2-dev \
    libfuse2 \
    docker.io \
    libssl-dev \
    binaryen \
    && rm -rf /var/lib/apt/lists/* \
    && rm -f /usr/share/glib-2.0/schemas/org.freedesktop.ColorHelper.gschema.xml

RUN rustup target add wasm32-unknown-unknown
RUN curl -fsSL https://deb.nodesource.com/setup_lts.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*
RUN npm install -g tailwindcss @tailwindcss/cli

RUN curl -L --proto '=https' --tlsv1.2 -sSf https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
RUN cargo install cross --locked
RUN cargo install cargo-deb --locked
RUN cargo install cargo-generate-rpm --locked
RUN cargo install cargo-packager --locked
RUN cargo install tauri-cli --locked
RUN cargo install dioxus-cli --locked
RUN cargo install cargo-make --locked

