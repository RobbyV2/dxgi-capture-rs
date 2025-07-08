//! Integration tests for dxgi-capture-rs library
//!
//! These tests verify end-to-end functionality of the DXGI capture system.

use dxgi_capture_rs::{CaptureError, DXGIManager};

/// Test complete capture workflow
#[test]
fn test_capture_workflow() {
    // Create manager
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("Could not create DXGI manager - may be running in headless environment");
            return;
        }
    };

    // Get screen geometry
    let (width, height) = manager.geometry();
    assert!(
        width > 0 && height > 0,
        "Screen dimensions should be positive"
    );

    // Attempt to capture a frame
    match manager.capture_frame() {
        Ok((pixels, (frame_width, frame_height))) => {
            assert_eq!(width, frame_width, "Frame width should match geometry");
            assert_eq!(height, frame_height, "Frame height should match geometry");
            assert_eq!(
                pixels.len(),
                width * height,
                "Pixel count should match dimensions"
            );

            // Verify pixel data structure
            for pixel in pixels.iter().take(10) {
                // BGRA values are u8, so they're automatically in valid range
                // Just verify the structure is accessible
                let _components = (pixel.b, pixel.g, pixel.r, pixel.a);
            }
        }
        Err(CaptureError::Timeout) => {
            println!("Capture timed out - acceptable in test environment");
        }
        Err(e) => {
            println!("Capture failed with error: {:?} - may be expected in CI", e);
        }
    }
}

/// Test capture source switching
#[test]
fn test_capture_source_switching() {
    let mut manager = match DXGIManager::new(500) {
        Ok(m) => m,
        Err(_) => {
            println!("Could not create DXGI manager");
            return;
        }
    };

    // Test switching between sources
    let original_index = manager.get_capture_source_index();
    assert_eq!(original_index, 0);

    // Try switching to different sources
    for i in 0..3 {
        manager.set_capture_source_index(i);
        let current_index = manager.get_capture_source_index();
        assert_eq!(current_index, i);

        // Try to capture from this source
        match manager.capture_frame() {
            Ok(_) => {
                println!("Successfully captured from source {}", i);
            }
            Err(e) => {
                println!("Could not capture from source {}: {:?}", i, e);
            }
        }
    }
}

/// Test timeout behavior
#[test]
fn test_timeout_behavior() {
    let mut manager = match DXGIManager::new(0) {
        Ok(m) => m,
        Err(_) => {
            println!("Could not create DXGI manager");
            return;
        }
    };

    // With 0ms timeout, capture might timeout immediately
    match manager.capture_frame() {
        Ok(_) => {
            println!("Capture succeeded despite 0ms timeout");
        }
        Err(CaptureError::Timeout) => {
            println!("Got expected timeout with 0ms timeout");
        }
        Err(e) => {
            println!("Got other error with 0ms timeout: {:?}", e);
        }
    }

    // Test changing timeout
    manager.set_timeout_ms(1000);
    match manager.capture_frame() {
        Ok(_) => {
            println!("Capture succeeded with 1000ms timeout");
        }
        Err(e) => {
            println!("Capture failed with 1000ms timeout: {:?}", e);
        }
    }
}

/// Test resource cleanup and multiple manager instances
#[test]
fn test_multiple_managers() {
    let mut managers = Vec::new();

    // Create multiple managers
    for i in 0..3 {
        match DXGIManager::new(500) {
            Ok(manager) => {
                managers.push(manager);
                println!("Created manager {}", i);
            }
            Err(_) => {
                println!("Could not create manager {} - may be resource limited", i);
                break;
            }
        }
    }

    // Try to capture from each manager
    for (i, manager) in managers.iter_mut().enumerate() {
        match manager.capture_frame() {
            Ok(_) => {
                println!("Manager {} captured successfully", i);
            }
            Err(e) => {
                println!("Manager {} capture failed: {:?}", i, e);
            }
        }
    }

    // Managers will be dropped here, testing cleanup
}

/// Test frame data consistency over time
#[test]
fn test_frame_consistency() {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("Could not create DXGI manager");
            return;
        }
    };

    let mut frame_dimensions = Vec::new();

    // Capture multiple frames and check dimension consistency
    for i in 0..5 {
        match manager.capture_frame() {
            Ok((_, dimensions)) => {
                frame_dimensions.push(dimensions);
                println!("Frame {}: {}x{}", i, dimensions.0, dimensions.1);
            }
            Err(e) => {
                println!("Frame {} failed: {:?}", i, e);
            }
        }
    }

    // All successful captures should have same dimensions
    if !frame_dimensions.is_empty() {
        let first_dim = frame_dimensions[0];
        for (i, &dim) in frame_dimensions.iter().enumerate() {
            assert_eq!(
                dim, first_dim,
                "Frame {} dimensions should match first frame",
                i
            );
        }
    }
}

/// Test error handling robustness
#[test]
fn test_error_handling_robustness() {
    // Test with various timeout values
    let timeout_values = vec![0, 1, 100, 1000, 5000];

    for timeout in timeout_values {
        match DXGIManager::new(timeout) {
            Ok(mut manager) => {
                println!("Created manager with {}ms timeout", timeout);

                // Try a quick capture
                match manager.capture_frame() {
                    Ok(_) => println!("Capture succeeded with {}ms timeout", timeout),
                    Err(e) => println!("Capture failed with {}ms timeout: {:?}", timeout, e),
                }
            }
            Err(_) => {
                println!("Could not create manager with {}ms timeout", timeout);
            }
        }
    }
}
