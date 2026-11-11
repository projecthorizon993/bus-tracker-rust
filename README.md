# Bus Tracker Rust

Track any bus with high-accuracy station-to-station ETA estimation.
Built in Rust, compiled on Windows PC first, cross-platform ready.

## Providers (from decompiled apps)

| Provider | Region | Endpoint | Key | Status |
|----------|--------|----------|-----|--------|
| VinBus | Hanoi (`hn`) | `api.busmap.vn` / `vbcore-api.vinbus.vn` | `auth-key` extracted | ⚠️ Endpoint may return 404 — demo fallback active |
| VinBus | Ho Chi Minh City (`hcm`) | `api.busmap.vn` / `vbcore-api.vinbus.vn` | `auth-key` extracted | ⚠️ Endpoint may return 404 — demo fallback active |
| GreenSM / Xanh SM | All | `api.gsm-api.net` | `x-api-key` extracted | ⚠️ Returns 401 Unauthorized — demo fallback active |
| **iHanoi Traffic Cameras** | **Hanoi (`hn`)** | **`cds.hanoi.gov.vn`** | **public (free)** | ✅ Working |

BusMap (`api.busmap.vn`, `api.busmap.city`) is used for live bus GPS, route info, and station ETA — endpoints extracted from the decompiled BusMap app.

### Demo Fallback

When live APIs are unreachable (VinBus 404, GreenSM 401, network issues), the app automatically falls back to **demo mode** with sample Hanoi and Ho Chi Minh City stations and buses. The desktop UI shows a `▲ DEMO (API unreachable)` badge in the status bar when demo data is active. All ETA estimation, congestion, and route planning features work in demo mode.

## Build & Run (Windows PC)

### Prerequisites

Install **Visual Studio Build Tools** with the C++ workload (needed for the MSVC linker):
https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022

### Build

```bash
# Clone
git clone <your-repo-url>
cd bus-tracker-rust

# Build release
cargo build --release

# Run CLI
cargo run --release -p tracker-cli
```

Release `.exe` binaries appear at:
- `target/release/tracker-cli.exe` — command-line interface
- `target/release/tracker-desktop.exe` — desktop GUI (egui, recommended)

## CLI Usage

### Track a route by number (shows all physical buses)

```bash
# Track route 60B in Hanoi — no API key needed
cargo run --release -p tracker-cli -- plan 60B hn
```

### List providers

```bash
cargo run --release -p tracker-cli -- providers
```

### Fetch live buses

```bash
cargo run --release -p tracker-cli -- buses --provider vinbus --region hn
```

### Station-to-station ETA

```bash
cargo run --release -p tracker-cli -- eta --provider busmap --region hn --from stop_0 --to stop_5
```

## Output Example

```
=== Route 60B — 14 stations ===

Live buses: 4

════════════════════════════════════════════════════════════════════════════════
Route 60B — Live ETA Board (region: hn)
════════════════════════════════════════════════════════════════════════════════
Bus      Next Stop                  ETA      Conf       Speed    Auth GPS    Auth Route   
────     ────────                   ───      ────       ─────    ────────    ────────   
60B-01   Lê Lợi                     6 min    High       32.0     ✓ fresh     ✓ on-route  
60B-07   Ba Đình                    11 min   Medium     18.0     ✓ fresh     ✓ on-route  
60B-14   Cầu Giấy                   9 min    Low        0.0      ✗ stale     ⚠ off-route 
60B-22   Nguyễn Trãi                14 min   High       28.0     ✓ fresh     ✓ on-route  
════════════════════════════════════════════════════════════════════════════════
```

Each row is a **different physical vehicle** on the same route number.

## Authenticity Checks

| Check | What it verifies |
|-------|------------------|
| **Auth GPS** | GPS fix is fresh (< 5 min old) |
| **Auth Route** | Bus is within 500m of route polyline or station |
| **Confidence** | High/Medium/Low based on data freshness + historical samples |

## Congestion Data (Free, No API Key)

Congestion data comes from **iHanoi** (`cds.hanoi.gov.vn`) — Hanoi's official traffic camera system. No Google Maps API key required.

## GitHub Actions

Push to `main` triggers a Windows build via GitHub Actions. Download `tracker-cli.exe`, `tracker-desktop.exe`, and `tracker-web.exe` from the build artifacts or GitHub Releases (tagged `v*`).

## Cross-Platform

```bash
rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target x86_64-unknown-linux-gnu
```