# Minna EP-1

**City Pop Hardware Control Center**

A standalone macOS prototype app for the Minna context engine with a "City Pop Hardware" aesthetic — 1980s Sony ES series meets Teenage Engineering.

## Design Philosophy

"Night Drive" aesthetic featuring:
- **Platinum Chassis**: High-fidelity silver body (`#F5F5FA`)
- **VFD Displays**: Midnight blue screens with cyan phosphor glow
- **Tactile Controls**: Hardware-inspired buttons and 1px borders
- **Obi Strips**: Japanese vinyl-style color accents per provider

## Quick Start

### Generate Xcode Project

```bash
cd src/minna_ep1
xcodegen generate
open MinnaEP1.xcodeproj
```

### Requirements

- macOS 14.0+
- Xcode 15.0+
- [XcodeGen](https://github.com/yonaskolb/XcodeGen) (for project generation)

Install XcodeGen:
```bash
brew install xcodegen
```

## Project Structure

```
minna_ep1/
├── MinnaEP1App.swift       # App entry point
├── MinnaEngineManager.swift # Engine/sync management
├── KeychainHelper.swift     # Secure token storage
├── Theme/
│   └── CityPopTheme.swift   # Colors, fonts, modifiers
├── Views/
│   ├── ControlCenterView.swift  # Main chassis layout
│   ├── TEProviderRow.swift      # Provider "cartridge"
│   ├── TEMemoryView.swift       # VFD tape monitor
│   └── TEModifiers.swift        # Blinking, buttons, etc.
├── Resources/
│   └── (icons)
├── project.yml             # XcodeGen config
├── Info.plist
└── MinnaEP1.entitlements
```

## Features

### Sources View (01. SOURCES)
- Provider cards with obi strip accents
- Real-time sync status with "REC" indicators
- Tactile sync/cancel buttons

### Memory View (02. MEMORY)
- VFD-style tape monitor
- Phosphor glow text effect
- Blinking cursor animation

### Config View (03. CONFIG)
- Coming soon

## Color Palette

| Name | Value | Usage |
|------|-------|-------|
| Chassis | `rgb(245, 245, 250)` | Main body |
| VFD Background | `rgb(13, 20, 38)` | Display screens |
| VFD Text | `rgb(0, 255, 230)` | Phosphor glow |
| Hot Pink | `rgb(255, 51, 179)` | Actions/errors |
| Slack Obi | `rgb(128, 0, 128)` | Deep Purple |
| Linear Obi | `rgb(255, 230, 0)` | Sun Yellow |
| GitHub Obi | `rgb(0, 51, 204)` | Cobalt Blue |

## Comparison with Main App

| Feature | swift_reference (Main) | minna_ep1 (Prototype) |
|---------|----------------------|----------------------|
| UI Style | Menu bar popover | Windowed app |
| Aesthetic | City Pop vinyl | City Pop Hardware |
| Window Size | 320×440 | 800×500 |
| Title Bar | Standard | Hidden |
| Target | Production | Design exploration |

## Development Notes

- This is a **standalone prototype** — changes here don't affect the main app
- The `MinnaEngineManager` is duplicated to allow independent iteration
- OAuth deep links (`minna://`) work the same as the main app





