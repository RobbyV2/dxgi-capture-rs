# dxgi-capture-rs

High-performance screen capturing with DXGI Desktop Duplication API for Windows in Rust.

[![Crate](https://img.shields.io/crates/v/dxgi-capture-rs.svg)](https://crates.io/crates/dxgi-capture-rs/)
[![Documentation](https://docs.rs/dxgi-capture-rs/badge.svg)](https://docs.rs/dxgi-capture-rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

This library provides a high-performance Rust interface to the Windows DXGI Desktop Duplication API, allowing you to capture screen content efficiently. It's designed for applications that need real-time screen capture capabilities with minimal performance overhead.

## Features

- **High Performance**: Direct access to DXGI Desktop Duplication API
- **Multiple Monitor Support**: Capture from any available display
- **Flexible Output**: Get pixel data as BGRA8 or raw component bytes
- **Frame Metadata**: Access dirty rectangles, moved rectangles, and timing information
- **Error Handling**: Comprehensive error types for robust applications
- **Windows Only**: Optimized specifically for Windows platforms

## Example

```rust
use dxgi_capture_rs::{DXGIManager, CaptureError};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new DXGI manager with 1000ms timeout
    let mut manager = DXGIManager::new(1000)?;
    
    // Get screen dimensions
    let (width, height) = manager.geometry();
    println!("Screen: {}x{}", width, height);
    
    // Capture a frame
    match manager.capture_frame() {
        Ok((pixels, (frame_width, frame_height))) => {
            println!("Captured {}x{} frame with {} pixels", 
                     frame_width, frame_height, pixels.len());
            
            // Process your pixel data here
            // pixels is Vec<BGRA8> where each pixel has b, g, r, a components
        }
        Err(CaptureError::Timeout) => {
            println!("Capture timed out - no new frame available");
        }
        Err(e) => {
            eprintln!("Capture failed: {:?}", e);
        }
    }
    
    Ok(())
}
```

## Frame Metadata for Streaming Applications

The library provides detailed frame metadata including dirty rectangles and moved rectangles, which is crucial for optimizing streaming and remote desktop applications. This allows you to only process and transmit the parts of the screen that have actually changed.

```rust
use dxgi_capture_rs::{DXGIManager, CaptureError};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut manager = DXGIManager::new(1000)?;

    match manager.capture_frame_with_metadata() {
        Ok((pixels, (width, height), metadata)) => {
            // Only process frame if there are actual changes
            if metadata.has_updates() {
                println!("Frame has {} dirty rects and {} move rects",
                         metadata.dirty_rects.len(), metadata.move_rects.len());
                
                // Process moved rectangles first (as per Microsoft recommendation)
                for move_rect in &metadata.move_rects {
                    let (src_x, src_y) = move_rect.source_point;
                    let (dst_left, dst_top, dst_right, dst_bottom) = move_rect.destination_rect;
                    
                    // Copy pixels from source to destination
                    // This is much more efficient than re-encoding the entire area
                    println!("Content moved from ({}, {}) to ({}, {}, {}, {})",
                             src_x, src_y, dst_left, dst_top, dst_right, dst_bottom);
                }
                
                // Then process dirty rectangles
                for &(left, top, right, bottom) in &metadata.dirty_rects {
                    let width = (right - left) as usize;
                    let height = (bottom - top) as usize;
                    
                    // Only encode/transmit the changed region
                    println!("Dirty region: ({}, {}) to ({}, {}) [{}x{}]",
                             left, top, right, bottom, width, height);
                }
            }
            
            // Check for mouse cursor updates
            if metadata.has_mouse_updates() {
                if let Some((x, y)) = metadata.pointer_position {
                    println!("Mouse cursor at ({}, {}), visible: {}", x, y, metadata.pointer_visible);
                }
            }
            
            // Access timing information
            if metadata.accumulated_frames > 1 {
                println!("This frame accumulated {} updates", metadata.accumulated_frames);
            }
        }
        Err(CaptureError::Timeout) => {
            println!("No new frame available");
        }
        Err(e) => {
            eprintln!("Capture failed: {:?}", e);
        }
    }
    
    Ok(())
}
```

### Benefits of Using Metadata

- **Bandwidth Optimization**: Only transmit changed regions for streaming applications
- **CPU Efficiency**: Avoid processing unchanged screen areas
- **Better User Experience**: Handle moved content more efficiently than dirty regions
- **Mouse Tracking**: Separate mouse cursor updates from screen content changes
- **Frame Timing**: Access Windows performance counters for precise timing

## API Reference

### DXGIManager

The main interface for screen capture operations.

#### Methods

- `new(timeout_ms: u32) -> Result<DXGIManager, &'static str>` - Create a new manager
- `geometry() -> (usize, usize)` - Get screen dimensions
- `capture_frame() -> Result<(Vec<BGRA8>, (usize, usize)), CaptureError>` - Capture a frame
- `capture_frame_components() -> Result<(Vec<u8>, (usize, usize)), CaptureError>` - Capture raw components
- `capture_frame_with_metadata() -> Result<(Vec<BGRA8>, (usize, usize), FrameMetadata), CaptureError>` - Capture with metadata
- `capture_frame_components_with_metadata() -> Result<(Vec<u8>, (usize, usize), FrameMetadata), CaptureError>` - Capture raw components with metadata
- `set_capture_source_index(index: usize)` - Select capture source (monitor)
- `set_timeout_ms(timeout_ms: u32)` - Update capture timeout

### Error Types

- `CaptureError::AccessDenied` - Could not duplicate output (protected content)
- `CaptureError::AccessLost` - Output duplication was lost (mode change)
- `CaptureError::RefreshFailure` - Could not refresh after failure
- `CaptureError::Timeout` - AcquireNextFrame timed out
- `CaptureError::Fail(msg)` - General failure with description

### Metadata Types

#### FrameMetadata

Contains frame information and change detection data:

- `dirty_rects: Vec<(i32, i32, i32, i32)>` - Changed screen regions (left, top, right, bottom)
- `move_rects: Vec<MoveRect>` - Moved screen regions
- `pointer_position: Option<(i32, i32)>` - Mouse cursor position if visible
- `pointer_visible: bool` - Whether mouse cursor is visible
- `last_present_time: i64` - Windows performance counter timestamp
- `last_mouse_update_time: i64` - Last mouse update timestamp
- `accumulated_frames: u32` - Number of frames accumulated since last processed
- `rects_coalesced: bool` - Whether rectangles were merged by the system
- `protected_content_masked_out: bool` - Whether protected content was hidden

#### Helper Methods

- `has_updates() -> bool` - Returns true if frame has any changes
- `has_mouse_updates() -> bool` - Returns true if mouse cursor was updated
- `total_change_count() -> usize` - Returns total number of changed regions

#### MoveRect

Represents content moved from one location to another:

- `source_point: (i32, i32)` - Source location (x, y)
- `destination_rect: (i32, i32, i32, i32)` - Destination rectangle (left, top, right, bottom)

## Multi-Monitor Support

```rust
let mut manager = DXGIManager::new(1000)?;

// Capture from primary monitor (index 0)
manager.set_capture_source_index(0);

// Capture from secondary monitor (index 1, if available)
manager.set_capture_source_index(1);
```

## Performance Considerations

- Use appropriate timeout values based on your frame rate needs
- Consider using `capture_frame_components()` if you need raw byte data
- Use metadata methods to optimize streaming by only processing changed regions
- Process move rectangles before dirty rectangles for correct visual output
- The library handles screen rotation automatically
- Memory usage scales with screen resolution

## System Requirements

- Windows 8 or later (DXGI 1.2+ required)
- Compatible graphics driver supporting Desktop Duplication
- Rust 1.88+ (edition 2024)

## Building

```bash
git clone https://github.com/RobbyV2/dxgi-capture-rs.git
cd dxgi-capture-rs
cargo build --release
```

This will build both the main library and the example application. You can also build just the library:

```bash
cargo build --release --package dxgi-capture-rs
```

## Example Application

The repository includes a complete example application that demonstrates real-time desktop streaming using `egui`. This example shows how to:

- Capture desktop frames at high performance
- Display the captured content in a resizable window
- Handle errors gracefully 
- Maintain aspect ratio when scaling

To run the example:

```bash
cargo run --package example-stream
```

The example application captures your desktop and displays it in a window with the following features:

- **Real-time streaming**: Captures desktop content as fast as possible
- **Resizable display**: The captured image scales to fit the window while maintaining aspect ratio
- **Error handling**: Shows informative error messages for capture failures
- **Performance optimized**: Only updates the display when new frames are available

Note: The example requires an active desktop session and may not work in headless environments.

## Testing

```bash
cargo test
```

Note: Tests may not run properly in headless environments (CI) as they require an active desktop session.

## License

This project is licensed under the MIT License.

See [LICENSE](./LICENSE) for details.

## Project Structure

This is a Cargo workspace containing:

- **`dxgi-capture-rs`** - The main library crate (published to crates.io)
- **`example-stream`** - Example application demonstrating real-time desktop streaming with egui (development only)

The workspace is configured so that:
- Both crates are built and tested together in CI
- Code formatting and linting applies to both crates
- The example application is excluded from publishing but included in development workflows
