# yt-dlp-GUI

A modern, cross-platform Graphical User Interface (GUI) for [yt-dlp](https://github.com/yt-dlp/yt-dlp), built with Tauri 2, React, and TypeScript.

The project aims to provide a simple and intuitive desktop application for downloading videos, images, GIFs, and audio from popular platforms such as YouTube, Instagram, X (Twitter), TikTok, and other websites supported by yt-dlp.

Instead of using command-line tools, users can paste a URL, select their preferred format and quality, and download media directly to their device.

## Tech Stack

| Technology                                    | Purpose                              |
| --------------------------------------------- | ------------------------------------ |
| [Tauri 2](https://v2.tauri.app/)              | Cross-platform application framework |
| [React](https://react.dev/)                   | User interface                       |
| [TypeScript](https://www.typescriptlang.org/) | Frontend development                 |
| [Rust](https://www.rust-lang.org/)            | Native backend integration           |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp)    | Media extraction and downloading     |
| [FFmpeg](https://ffmpeg.org/)                 | Media conversion and processing      |

## Supported Platforms

| Platform | Status                      |
| -------- | --------------------------- |
| Linux    | 🚧 In development           |
| Windows  | 📋 Planned                  |
| Android  | 📋 Planned                  |
| macOS    | 💡 Not Planned |

Mobile support will require additional work to integrate yt-dlp and FFmpeg.

## Planned Features

- **URL-based downloads** — Download media by pasting a supported URL.
- **Multiple platforms** — Support YouTube, Instagram, X, TikTok, and other yt-dlp-compatible websites.
- **Format selection** — Choose between available video, audio, and image formats.
- **Quality selection** — Select the preferred resolution and quality.
- **Media preview** — Display available metadata, including titles, thumbnails, and duration.
- **Download progress** — Track progress, download speed, and estimated completion time.
- **Audio extraction** — Extract audio from videos using FFmpeg.
- **GIF conversion** — Convert supported videos into animated GIFs.
- **Download management** — Manage multiple downloads and their status.
- **Custom download directory** — Choose where downloaded files are saved.
- **Cross-platform support** — Provide a consistent interface across supported operating systems.

*Features listed above are planned and may change during development.*

## Getting Started

### Prerequisites

To develop the application locally, install:

- Node.js and npm
- Rust and Cargo
- Tauri 2 system dependencies for your operating system
- yt-dlp
- FFmpeg

See the [Tauri prerequisites documentation](https://v2.tauri.app/start/prerequisites/) for platform-specific requirements.

### Installation

Clone the repository:

```bash
git clone https://github.com/HeHexa6ty/Tauri-yt-dlp-GUI
cd tauri-ytdlp-gui
```

Install dependencies:

```bash
npm install
```

Start the development application:

```bash
npm run tauri dev
```

### Production Build

Build the native application:

```bash
npm run tauri build
```

Builds are platform-specific and require the appropriate toolchain and dependencies.

## Project Structure

```text
yt-dlp-GUI/
├── src/                  # React + TypeScript frontend
├── src-tauri/            # Tauri configuration and Rust backend
├── public/               # Static assets
├── package.json
└── README.md
```

## How It Works

1. The user pastes a media URL into the application.
2. The application invokes yt-dlp to retrieve available media information.
3. The user selects the desired format, quality, and download directory.
4. yt-dlp downloads the selected media.
5. FFmpeg performs any required conversion or processing.
6. The completed file is saved locally.

All media processing is intended to run locally, without requiring a dedicated download server.

## Development Roadmap

- [ ] Set up Tauri 2 with React and TypeScript
- [ ] Design the main application interface
- [ ] Integrate yt-dlp with the Rust backend
- [ ] Implement URL analysis and media metadata retrieval
- [ ] Implement video and audio downloading
- [ ] Add quality and format selection
- [ ] Integrate FFmpeg for media conversion
- [ ] Implement download progress tracking
- [ ] Add download queue management
- [ ] Implement application settings
- [ ] Package the application for Linux
- [ ] Add Windows support
- [ ] Investigate Android support

## Disclaimer

This project is an independent graphical interface for yt-dlp and is not affiliated with YouTube, Instagram, X, TikTok, or their respective owners.

Users are responsible for ensuring that downloading and using media complies with applicable copyright laws, platform terms of service, and any other relevant restrictions.

## License

License to be determined.

---

**Project Status:** 🚧 Early Development
