# Bus Tracker Rust

Track any bus with high-accuracy station-to-station ETA estimation.
Built in Rust, compiled on Windows PC first, cross-platform ready.

## Providers (from decompiled apps)

| Provider | Region | Endpoint | Key |
|----------|--------|----------|-----|
| VinBus | Hanoi (`hn`) | `vbcore-api.vinbus.vn` | `auth-key` extracted |
| VinBus | Ho Chi Minh City (`hcm`) | `vbcore-api.vinbus.vn` | `auth-key` extracted |
| GreenSM / Xanh SM | All | `api.gsm-api.net` | `x-api-key` extracted |
| iHanoi Traffic Cameras | Hanoi | `cds.hanoi.gov.vn` | public |

## Build & Run (Windows PC)

```bash
# Clone
git clone <your-repo-url>
cd bus-tracker-rust

# Build release
cargo build --release

# Run desktop UI
cargo run --release -p tracker-desktop

# Or run CLI
cargo run --release -p tracker-cli
```

Release `.exe` appears at: `target/release/tracker-desktop.exe`

## Features

- Provider selection from list (all regions found in decompiled apps)
- Station-to-station ETA (origin → destination)
- Live bus board with speed, distance, ETA, confidence
- Congestion-aware estimation (live traffic + historical baseline)
- Auto-refreshing desktop UI (egui)
- Local cache for historical speeds

## Cross-Platform

```bash
rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target x86_64-unknown-linux-gnu
```