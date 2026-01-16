# Minna Development Setup: Web-Like Protocol

This guide explains how to set up the "Web-Like" development protocol for Minna macOS app development in Cursor, enabling hot reload and instant UI iteration.

## Overview

The Web-Like Protocol transforms native macOS development to match web development velocity by:

1. **InjectionNext**: Hot module replacement (HMR-like function swizzling)
2. **Sweetpad**: Build/run controls in Cursor (eliminates Xcode context switching)
3. **Mock-First Architecture**: Protocol abstraction for instant UI iteration

## Prerequisites

- macOS 14.0+
- Cursor IDE
- Xcode 15.0+ (for building, but you won't need to open it)

## Step 1: Install Sweetpad Extension

Sweetpad provides build/run/test controls directly in Cursor, similar to running `npm start` in VS Code.

### Installation

1. Open Cursor
2. Go to Extensions (Cmd+Shift+X)
3. Search for "Sweetpad"
4. Install the extension
5. Restart Cursor if prompted

### Configuration

Sweetpad will automatically detect your Xcode project. No additional configuration needed.

## Step 2: Install InjectionNext

InjectionNext enables hot reload for Swift code, swapping function bodies in the running app without a full rebuild.

### Installation

1. Download InjectionNext from: https://github.com/johnno1962/InjectionIII/releases
2. Install the app (drag to Applications)
3. Launch InjectionNext
4. It will automatically watch for changes in your running app

### Configuration

The linker flag `-Xlinker -interposable` has already been added to the Debug build configuration in `project.yml`. This enables function swizzling.

## Step 3: Running the App

### Using Sweetpad

1. Open the project in Cursor
2. Look for the Sweetpad sidebar panel (usually on the left)
3. Click the "Run" button (▶️) or press Cmd+R
4. Sweetpad will build and launch the app using `xcodebuild`

**Note**: First build may take 30-60 seconds. Subsequent builds are faster.

### Using Terminal (Alternative)

If Sweetpad isn't working, you can build manually:

```bash
cd minna-local-engine/src/minna_ep1
xcodegen generate
xcodebuild -scheme MinnaEP1 -configuration Debug
open build/Debug/MinnaEP1.app
```

## Step 4: Hot Reload Workflow

Once the app is running:

1. **Make a SwiftUI change** (e.g., change a color, text, or layout)
2. **Save the file** (Cmd+S)
3. **InjectionNext automatically detects** the change
4. **Function swizzling occurs** (<1 second)
5. **UI updates instantly** without rebuild

### What Works with Hot Reload

- SwiftUI view changes (colors, text, layout)
- Function body changes
- Property modifications
- Most Swift code changes

### What Requires Full Rebuild

- Adding new files
- Changing function signatures
- Protocol conformance changes
- Structural changes (new classes, enums)

## Step 5: Mock-First UI Development

For instant UI iteration without waiting for the Rust backend:

### Using SwiftUI Previews

1. Open any view file (e.g., `ControlCenterView.swift`)
2. Click the "Resume" button in the preview canvas (or press Cmd+Option+P)
3. Previews use `MockSignalEngine` by default
4. **Previews render in <100ms** (no Rust daemon needed)

### Mock States Available

```swift
// Sunny state - all providers active
MockSignalEngine(state: .sunny)

// Indexing state - one provider syncing
MockSignalEngine(state: .indexing)

// Welcome state - no providers connected
MockSignalEngine(state: .welcome)

// Error state - error condition
MockSignalEngine(state: .error)
```

### Switching to Real Engine

When you need to test Rust integration:

1. Change preview to use `RealSignalEngine()`:
   ```swift
   ControlCenterView(engine: RealSignalEngine())
   ```
2. Or run the app - it uses `RealSignalEngine()` by default

## Step 6: Visual Loop Workflow (Cursor as Inspector)

Use Cursor's multimodal capabilities as your inspector:

1. **Run app** with Sweetpad
2. **Take screenshot** of UI issue (Cmd+Shift+4)
3. **Paste into Cursor Composer** (Cmd+I)
4. **Prompt**: "Make this look like the Figma spec. Reference ChatView.swift."
5. **Cursor writes code** → InjectionNext hot-reloads
6. **See changes instantly** (<1 second)

## Troubleshooting

### InjectionNext Not Working

- Ensure the app was built with Debug configuration
- Check that `-Xlinker -interposable` is in Debug build settings
- Restart InjectionNext app
- Rebuild the app

### Sweetpad Not Detecting Project

- Ensure `project.yml` exists in `minna-local-engine/src/minna_ep1/`
- Run `xcodegen generate` manually first
- Check that Xcode command-line tools are installed: `xcode-select --install`

### Previews Not Rendering

- Ensure you're using `MockSignalEngine` in previews
- Check that preview code compiles (look for errors in Xcode preview panel)
- Try cleaning build folder: Cmd+Shift+K in Xcode

### Rust Daemon Issues

- If testing with `RealSignalEngine`, ensure Rust daemon is running
- Check `~/.minna/mcp.sock` exists
- Check logs in Console.app for daemon errors

## Development Workflow Summary

### Fast UI Iteration (Mock-First)
1. Open view in Cursor
2. Use SwiftUI Previews with `MockSignalEngine`
3. Iterate on layout/styling (<100ms feedback)
4. Hot reload with InjectionNext when app is running

### Integration Testing
1. Run app with Sweetpad
2. App uses `RealSignalEngine()` by default
3. Test Rust daemon integration
4. Use hot reload for Swift changes

### Visual Design Iteration
1. Run app → Screenshot → Cursor Composer
2. Cursor writes code → InjectionNext reloads
3. Instant visual feedback loop

## Performance Metrics

- **Preview Render Time**: <100ms (with MockSignalEngine)
- **Hot Reload Time**: <1 second (with InjectionNext)
- **Context Switching**: Zero (no Xcode needed)

## Next Steps

- Explore different `MockSignalEngine` states for edge cases
- Create custom mock scenarios for your specific UI needs
- Document common mock patterns for your team
