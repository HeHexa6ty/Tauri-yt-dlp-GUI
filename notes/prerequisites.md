## Step 1 — Install Tauri prerequisites
sudo pacman -Syu --needed \
    base-devel \
    curl \
    wget \
    file \
    openssl \
    appmenu-gtk-module \
    libappindicator-gtk3 \
    librsvg \
    webkit2gtk-4.1 \
    gtk3

### Verify
rustc --version
cargo --version
node --version
npm --version

### Official Rust Installer
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

## Step 2 — Create your Tauri project

npm create tauri-app@latest
