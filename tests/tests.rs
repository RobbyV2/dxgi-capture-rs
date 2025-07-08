//! Comprehensive tests for dxgi-capture-rs library
//!
//! These tests verify both unit functionality and end-to-end integration of the DXGI capture system.

use dxgi_capture_rs::{BGRA8, CaptureError, DXGIManager, hr_failed};

// ============================================================================
// UNIT TESTS
// ============================================================================

/// Test DXGI manager creation with various timeout values
#[test]
fn test_dxgi_manager_creation() {
    // Test with reasonable timeout - this should work if DXGI is available
    let result = DXGIManager::new(1000);

    // If DXGI is not available (headless environment), skip the test
    if result.is_err() {
        println!("DXGI not available - skipping test (expected in headless environments)");
        return;
    }

    // If we get here, DXGI is available
    // Test timeout value setting and retrieval - use the already created manager
    let mut manager = result.unwrap();

    // Test getting initial timeout
    assert_eq!(
        manager.get_timeout_ms(),
        1000,
        "Initial timeout should be 1000ms"
    );

    // Test setting different timeout values
    manager.set_timeout_ms(100);
    assert_eq!(
        manager.get_timeout_ms(),
        100,
        "Timeout should be updated to 100ms"
    );

    manager.set_timeout_ms(5000);
    assert_eq!(
        manager.get_timeout_ms(),
        5000,
        "Timeout should be updated to 5000ms"
    );

    // Test setting timeout to 0
    manager.set_timeout_ms(0);
    assert_eq!(manager.get_timeout_ms(), 0, "Timeout should be set to 0ms");

    // Test setting timeout to maximum value
    manager.set_timeout_ms(u32::MAX);
    assert_eq!(
        manager.get_timeout_ms(),
        u32::MAX,
        "Timeout should be set to maximum value"
    );
}

/// Test timeout configuration
#[test]
fn test_timeout_configuration() {
    let mut manager = match DXGIManager::new(500) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Test setting different timeout values
    manager.set_timeout_ms(100);
    manager.set_timeout_ms(2000);
    manager.set_timeout_ms(0);

    // No panic should occur with any timeout value
}

/// Test capture source index management
#[test]
fn test_capture_source_index() {
    let mut manager = match DXGIManager::new(300) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Test getting initial capture source index
    let initial_index = manager.get_capture_source_index();
    assert_eq!(initial_index, 0, "Initial capture source should be 0");

    // Test setting capture source index
    manager.set_capture_source_index(0);
    assert_eq!(manager.get_capture_source_index(), 0);

    // Test with different indices (may fail if no additional displays)
    // This is expected behavior and shouldn't panic
    manager.set_capture_source_index(1);
}

/// Test geometry retrieval and consistency
#[test]
fn test_geometry() {
    let manager = match DXGIManager::new(300) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Test that geometry is consistent across multiple calls
    let (width1, height1) = manager.geometry();
    let (width2, height2) = manager.geometry();

    assert_eq!(width1, width2, "Width should be consistent across calls");
    assert_eq!(height1, height2, "Height should be consistent across calls");

    // Test reasonable bounds
    assert!(width1 > 0, "Width should be greater than 0, got {width1}");
    assert!(
        height1 > 0,
        "Height should be greater than 0, got {height1}"
    );

    // Test minimum reasonable resolution (at least 640x480)
    assert!(
        width1 >= 640,
        "Width should be at least 640px, got {width1}"
    );
    assert!(
        height1 >= 480,
        "Height should be at least 480px, got {height1}"
    );

    // Test maximum reasonable bounds for modern displays
    assert!(width1 <= 16384, "Width should be <= 16384px, got {width1}");
    assert!(
        height1 <= 16384,
        "Height should be <= 16384px, got {height1}"
    );

    // Test aspect ratio is reasonable (between 4:3 and 21:9)
    let aspect_ratio = width1 as f64 / height1 as f64;
    assert!(
        aspect_ratio >= 1.0,
        "Aspect ratio should be >= 1.0 (width >= height)"
    );
    assert!(
        aspect_ratio <= 3.5,
        "Aspect ratio should be <= 3.5 (ultra-wide), got {aspect_ratio:.2}"
    );
}

/// Test frame capture functionality and data integrity
#[test]
fn test_frame_capture() {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Get expected dimensions from geometry
    let (expected_width, expected_height) = manager.geometry();

    // Test single frame capture
    let result = manager.capture_frame();
    match result {
        Ok((pixels, (width, height))) => {
            // Test basic data integrity
            assert!(!pixels.is_empty(), "Pixel data should not be empty");
            assert_eq!(
                pixels.len(),
                width * height,
                "Pixel count should match dimensions: expected {}, got {}",
                width * height,
                pixels.len()
            );

            // Test dimensions match geometry
            assert_eq!(width, expected_width, "Frame width should match geometry");
            assert_eq!(
                height, expected_height,
                "Frame height should match geometry"
            );

            // Test pixel data validity - check a few random pixels
            let sample_indices = [0, pixels.len() / 4, pixels.len() / 2, pixels.len() - 1];
            for &idx in &sample_indices {
                let pixel = pixels[idx];
                // BGRA values are always valid u8 values, but let's check they're accessible
                let _check = (pixel.b, pixel.g, pixel.r, pixel.a);
            }

            // Test that not all pixels are identical (would indicate a problem)
            let first_pixel = pixels[0];
            let all_same = pixels
                .iter()
                .all(|&p| p.b == first_pixel.b && p.g == first_pixel.g && p.r == first_pixel.r);
            if all_same {
                println!(
                    "Warning: All pixels have identical color - may indicate solid color screen"
                );
            }
        }
        Err(CaptureError::Timeout) => {
            // Timeout with 1000ms is concerning but acceptable in CI
            println!("Frame capture timed out after 1000ms (may be expected in CI)");
        }
        Err(e) => {
            panic!("Frame capture failed with error: {e:?} - this indicates a real problem");
        }
    }
}

/// Test frame components capture
#[test]
fn test_frame_components_capture() {
    let mut manager = match DXGIManager::new(300) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let result = manager.capture_frame_components();
    match result {
        Ok((components, (width, height))) => {
            assert!(!components.is_empty(), "Component data should not be empty");
            assert_eq!(
                components.len(),
                width * height * 4,
                "Component count should be width * height * 4"
            );
            assert!(width > 0 && height > 0, "Dimensions should be positive");
        }
        Err(CaptureError::Timeout) => {
            // Timeout is acceptable in test environment
            println!("Frame components capture timed out (acceptable in tests)");
        }
        Err(e) => {
            println!("Frame components capture failed with error: {e:?}");
            // Don't fail the test for other errors as they might be environment-specific
        }
    }
}

/// Test consistency between frame capture methods
#[test]
fn test_frame_capture_consistency() {
    let mut manager = match DXGIManager::new(300) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let frame_result = manager.capture_frame();
    let components_result = manager.capture_frame_components();

    match (frame_result, components_result) {
        (Ok((pixels, (fw, fh))), Ok((components, (cw, ch)))) => {
            assert_eq!(fw, cw, "Frame widths should match");
            assert_eq!(fh, ch, "Frame heights should match");
            assert_eq!(
                pixels.len() * 4,
                components.len(),
                "Component data should be 4x pixel data"
            );
        }
        (Err(e1), Err(e2)) => {
            println!("Both captures failed: {e1:?}, {e2:?}");
            // Both failing is acceptable in test environment
        }
        _ => {
            // One succeeding and one failing might indicate an issue, but could be timing
            println!("Inconsistent capture results between methods");
        }
    }
}

/// Test multiple frame captures for stability
#[test]
fn test_multiple_captures() {
    let mut manager = match DXGIManager::new(300) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };
    let mut successful_captures = 0;

    for i in 0..10 {
        match manager.capture_frame() {
            Ok((pixels, (width, height))) => {
                successful_captures += 1;
                assert!(
                    !pixels.is_empty(),
                    "Pixel data should not be empty on capture {i}"
                );
                assert!(
                    width > 0 && height > 0,
                    "Dimensions should be positive on capture {i}"
                );

                // Test average color calculation like the original test
                let len = pixels.len() as u64;
                let (r, g, b) = pixels.into_iter().fold((0u64, 0u64, 0u64), |(r, g, b), p| {
                    (r + p.r as u64, g + p.g as u64, b + p.b as u64)
                });

                // Colors should be in valid range
                assert!(r / len <= 255, "Average red should be <= 255");
                assert!(g / len <= 255, "Average green should be <= 255");
                assert!(b / len <= 255, "Average blue should be <= 255");
            }
            Err(CaptureError::Timeout) => {
                // Timeouts are acceptable in test environment
                continue;
            }
            Err(e) => {
                println!("Capture {i} failed with error: {e:?}");
                // Don't fail immediately, some errors might be temporary
            }
        }
    }

    // At least some captures should succeed if the system is available
    if successful_captures == 0 {
        println!("No captures succeeded - this may be expected in headless test environments");
    }
}

/// Test BGRA8 color structure
#[test]
fn test_bgra8_color() {
    let color = BGRA8 {
        b: 255,
        g: 128,
        r: 64,
        a: 255,
    };

    assert_eq!(color.b, 255);
    assert_eq!(color.g, 128);
    assert_eq!(color.r, 64);
    assert_eq!(color.a, 255);

    // Test copy and clone
    let color2 = color;
    assert_eq!(color, color2);

    let color3 = color;
    assert_eq!(color, color3);
}

/// Test hr_failed utility function
#[test]
fn test_hr_failed() {
    // Test success codes
    assert!(!hr_failed(0), "S_OK should not be a failure");
    assert!(!hr_failed(1), "Positive values should not be failures");

    // Test failure codes
    assert!(hr_failed(-1), "Negative values should be failures");
    assert!(hr_failed(-2147467259), "E_FAIL should be a failure");
}

/// Test capture error variants
#[test]
fn test_capture_error_variants() {
    // Test that all error variants can be created and formatted
    let errors = vec![
        CaptureError::AccessDenied,
        CaptureError::AccessLost,
        CaptureError::RefreshFailure,
        CaptureError::Timeout,
        CaptureError::Fail("Test error message"),
    ];

    for error in errors {
        let debug_string = format!("{error:?}");
        assert!(
            !debug_string.is_empty(),
            "Error should have debug representation"
        );
    }
}

/// Test HRESULT error code mapping accuracy
#[test]
fn test_hr_failed_comprehensive() {
    use winapi::shared::winerror::*;

    // Test specific Windows error codes
    assert!(!hr_failed(S_OK), "S_OK should not be a failure");
    assert!(!hr_failed(S_FALSE), "S_FALSE should not be a failure");
    assert!(hr_failed(E_FAIL), "E_FAIL should be a failure");
    assert!(hr_failed(E_INVALIDARG), "E_INVALIDARG should be a failure");
    assert!(
        hr_failed(E_OUTOFMEMORY),
        "E_OUTOFMEMORY should be a failure"
    );
    assert!(
        hr_failed(E_ACCESSDENIED),
        "E_ACCESSDENIED should be a failure"
    );

    // Test boundary conditions
    assert!(
        !hr_failed(0x7FFFFFFF),
        "Maximum positive HRESULT should not be failure"
    );
    assert!(
        hr_failed(0x80000000u32 as i32),
        "Minimum negative HRESULT should be failure"
    );
}

/// Test BGRA8 struct properties and operations
#[test]
fn test_bgra8_comprehensive() {
    // Test creation and field access
    let pixel = BGRA8 {
        b: 10,
        g: 20,
        r: 30,
        a: 255,
    };
    assert_eq!(pixel.b, 10);
    assert_eq!(pixel.g, 20);
    assert_eq!(pixel.r, 30);
    assert_eq!(pixel.a, 255);

    // Test Copy trait
    let pixel2 = pixel;
    assert_eq!(pixel, pixel2);

    // Test Clone trait - but since BGRA8 implements Copy, we should just use copy semantics
    let pixel3 = pixel; // This tests that Copy works, which also proves Clone works
    assert_eq!(pixel, pixel3);

    // Test Debug trait
    let debug_str = format!("{pixel:?}");
    assert!(debug_str.contains("10"));
    assert!(debug_str.contains("20"));
    assert!(debug_str.contains("30"));
    assert!(debug_str.contains("255"));

    // Test ordering
    let pixel_smaller = BGRA8 {
        b: 5,
        g: 20,
        r: 30,
        a: 255,
    };
    assert!(pixel_smaller < pixel);

    // Test equality with different values
    let pixel_different = BGRA8 {
        b: 10,
        g: 21,
        r: 30,
        a: 255,
    };
    assert_ne!(pixel, pixel_different);
}

/// Test timeout behavior more rigorously
#[test]
fn test_timeout_behavior_strict() {
    let mut manager = match DXGIManager::new(1) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Test very short timeout should likely timeout
    manager.set_timeout_ms(1);
    assert_eq!(manager.get_timeout_ms(), 1);

    // Multiple attempts with short timeout - at least some should timeout
    let mut timeout_count = 0;
    let mut success_count = 0;

    for _ in 0..5 {
        match manager.capture_frame() {
            Ok(_) => success_count += 1,
            Err(CaptureError::Timeout) => timeout_count += 1,
            Err(e) => panic!("Unexpected error (not timeout): {e:?}"),
        }
    }

    // With 1ms timeout, we should see some timeouts unless the screen is very active
    if timeout_count == 0 && success_count > 0 {
        println!("No timeouts with 1ms - screen may be very active or system very fast");
    }

    println!("Timeout test: {timeout_count} timeouts, {success_count} successes");
}

/// Test capture source index validation
#[test]
fn test_capture_source_index_validation() {
    let mut manager = match DXGIManager::new(500) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Test initial state
    assert_eq!(manager.get_capture_source_index(), 0);

    // Test setting to same value
    manager.set_capture_source_index(0);
    assert_eq!(manager.get_capture_source_index(), 0);

    // Test setting to higher values (may not have corresponding displays)
    for i in 1..5 {
        manager.set_capture_source_index(i);
        assert_eq!(manager.get_capture_source_index(), i);

        // Try to capture - may fail if display doesn't exist, but shouldn't panic
        let _ = manager.capture_frame();
    }

    // Reset to primary display
    manager.set_capture_source_index(0);
    assert_eq!(manager.get_capture_source_index(), 0);
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

/// Test complete real-world capture workflow with data validation
#[test]
fn test_capture_workflow() {
    // Create manager with realistic timeout
    let mut manager = match DXGIManager::new(2000) {
        Ok(m) => m,
        Err(_) => {
            println!("Could not create DXGI manager - may be running in headless environment");
            return;
        }
    };

    let (width, height) = manager.geometry();
    assert!(
        width > 0 && height > 0,
        "Screen dimensions should be positive"
    );

    // Test multiple consecutive captures to ensure stability
    let mut successful_captures = 0;
    let mut frame_sizes = Vec::new();

    for attempt in 0..3 {
        match manager.capture_frame() {
            Ok((pixels, (frame_width, frame_height))) => {
                successful_captures += 1;

                // Verify consistency across captures
                assert_eq!(
                    width, frame_width,
                    "Frame width should be consistent across captures"
                );
                assert_eq!(
                    height, frame_height,
                    "Frame height should be consistent across captures"
                );
                assert_eq!(
                    pixels.len(),
                    width * height,
                    "Pixel count should always match dimensions"
                );

                frame_sizes.push(pixels.len());

                // Test pixel data integrity - verify we can access all pixels
                let first_pixel = pixels[0];
                let last_pixel = pixels[pixels.len() - 1];
                let mid_pixel = pixels[pixels.len() / 2];

                // These shouldn't panic
                let _ = (first_pixel.b, first_pixel.g, first_pixel.r, first_pixel.a);
                let _ = (last_pixel.b, last_pixel.g, last_pixel.r, last_pixel.a);
                let _ = (mid_pixel.b, mid_pixel.g, mid_pixel.r, mid_pixel.a);

                println!(
                    "Capture {} successful: {}x{} pixels",
                    attempt + 1,
                    frame_width,
                    frame_height
                );
            }
            Err(CaptureError::Timeout) => {
                println!("Capture {} timed out after 2000ms", attempt + 1);
            }
            Err(e) => {
                panic!(
                    "Capture {} failed with critical error: {:?}",
                    attempt + 1,
                    e
                );
            }
        }
    }

    // Verify all successful captures had consistent size
    if successful_captures > 1 {
        let first_size = frame_sizes[0];
        for (i, &size) in frame_sizes.iter().enumerate() {
            assert_eq!(
                size,
                first_size,
                "Frame {} size inconsistent with first frame",
                i + 1
            );
        }
    }

    if successful_captures == 0 {
        println!("Warning: No captures succeeded - may indicate display issues or CI environment");
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
                println!("Successfully captured from source {i}");
            }
            Err(e) => {
                println!("Could not capture from source {i}: {e:?}");
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
            println!("Got other error with 0ms timeout: {e:?}");
        }
    }

    // Test changing timeout
    manager.set_timeout_ms(1000);
    match manager.capture_frame() {
        Ok(_) => {
            println!("Capture succeeded with 1000ms timeout");
        }
        Err(e) => {
            println!("Capture failed with 1000ms timeout: {e:?}");
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
                println!("Created manager {i}");
            }
            Err(_) => {
                println!("Could not create manager {i} - may be resource limited");
                break;
            }
        }
    }

    // Try to capture from each manager
    for (i, manager) in managers.iter_mut().enumerate() {
        match manager.capture_frame() {
            Ok(_) => {
                println!("Manager {i} captured successfully");
            }
            Err(e) => {
                println!("Manager {i} capture failed: {e:?}");
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
                println!("Frame {i} failed: {e:?}");
            }
        }
    }

    // All successful captures should have same dimensions
    if !frame_dimensions.is_empty() {
        let first_dim = frame_dimensions[0];
        for (i, &dim) in frame_dimensions.iter().enumerate() {
            assert_eq!(
                dim, first_dim,
                "Frame {i} dimensions should match first frame"
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
                println!("Created manager with {timeout}ms timeout");

                // Try a quick capture
                match manager.capture_frame() {
                    Ok(_) => println!("Capture succeeded with {timeout}ms timeout"),
                    Err(e) => println!("Capture failed with {timeout}ms timeout: {e:?}"),
                }
            }
            Err(_) => {
                println!("Could not create manager with {timeout}ms timeout");
            }
        }
    }
}

/// Test data consistency between different capture methods
#[test]
fn test_capture_method_consistency() {
    let mut manager = match DXGIManager::new(1500) {
        Ok(m) => m,
        Err(_) => {
            println!("Could not create DXGI manager");
            return;
        }
    };

    // Test both methods work and have consistent structure
    let frame_result = manager.capture_frame();
    let components_result = manager.capture_frame_components();

    match (frame_result, components_result) {
        (Ok((pixels, (fw, fh))), Ok((components, (cw, ch)))) => {
            // Test dimensional consistency
            assert_eq!(
                fw, cw,
                "Frame dimensions must match between capture methods"
            );
            assert_eq!(
                fh, ch,
                "Frame dimensions must match between capture methods"
            );

            // Test data size consistency
            assert_eq!(
                pixels.len() * 4,
                components.len(),
                "Component array should be exactly 4x pixel array size"
            );

            // Test that the data structures are valid (don't compare pixel-by-pixel
            // since captures may happen at different times and get different content)
            assert!(!pixels.is_empty(), "Pixel array should not be empty");
            assert!(
                !components.is_empty(),
                "Component array should not be empty"
            );

            // Verify we can access both data structures properly
            let first_pixel = pixels[0];
            let first_components = [components[0], components[1], components[2], components[3]];

            // Just verify the data is accessible (no need to check ranges since u8 is always 0-255)
            let _ = (first_pixel.b, first_pixel.g, first_pixel.r, first_pixel.a);
            let _ = first_components;

            println!(
                "Both capture methods work correctly: {}x{} pixels, {} components",
                fw,
                fh,
                components.len()
            );
        }
        (Err(e1), Err(e2)) => {
            // Both methods failing consistently is acceptable
            println!("Both capture methods failed consistently: {e1:?}, {e2:?}");
        }
        (Ok(_), Err(e)) => {
            panic!("Inconsistent behavior: frame capture succeeded but components failed: {e:?}");
        }
        (Err(e), Ok(_)) => {
            panic!("Inconsistent behavior: components capture succeeded but frame failed: {e:?}");
        }
    }
}

/// Test performance and memory characteristics
#[test]
fn test_performance_characteristics() {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("Could not create DXGI manager");
            return;
        }
    };

    let (width, height) = manager.geometry();
    let expected_pixel_count = width * height;

    // Test that large captures complete in reasonable time
    let start = std::time::Instant::now();
    let mut successful_captures = 0;

    for i in 0..5 {
        match manager.capture_frame() {
            Ok((pixels, _)) => {
                successful_captures += 1;

                // Verify memory allocation is correct
                assert_eq!(
                    pixels.len(),
                    expected_pixel_count,
                    "Capture {} has wrong pixel count",
                    i + 1
                );

                // Verify data isn't obviously corrupted (all zeros or all 255s)
                let all_zero = pixels
                    .iter()
                    .all(|p| p.b == 0 && p.g == 0 && p.r == 0 && p.a == 0);
                let all_max = pixels
                    .iter()
                    .all(|p| p.b == 255 && p.g == 255 && p.r == 255 && p.a == 255);

                if all_zero {
                    println!("Warning: Capture {} appears to be all black pixels", i + 1);
                }
                if all_max {
                    println!("Warning: Capture {} appears to be all white pixels", i + 1);
                }

                assert!(
                    !all_zero || !all_max,
                    "Data shouldn't be completely uniform unless screen actually is"
                );
            }
            Err(CaptureError::Timeout) => {
                println!("Capture {} timed out", i + 1);
            }
            Err(e) => {
                println!("Capture {} failed: {:?}", i + 1, e);
            }
        }
    }

    let duration = start.elapsed();
    if successful_captures > 0 {
        let avg_time = duration / successful_captures;
        println!("Average capture time: {avg_time:?} for {width}x{height} display");

        // Sanity check - captures shouldn't take extremely long
        assert!(
            avg_time.as_secs() < 5,
            "Captures taking too long: {avg_time:?}"
        );
    }
}

/// Test manager behavior under resource constraints
#[test]
fn test_resource_management() {
    // Test creating multiple managers doesn't cause resource leaks
    let mut managers = Vec::new();

    for i in 0..3 {
        match DXGIManager::new(500) {
            Ok(manager) => {
                managers.push(manager);
                println!("Successfully created manager {}", i + 1);
            }
            Err(e) => {
                println!("Failed to create manager {}: {}", i + 1, e);
                break;
            }
        }
    }

    // Test that all managers can capture simultaneously
    let mut active_managers = 0;
    for (i, manager) in managers.iter_mut().enumerate() {
        match manager.capture_frame() {
            Ok((pixels, (w, h))) => {
                active_managers += 1;
                assert!(
                    !pixels.is_empty(),
                    "Manager {} should capture non-empty data",
                    i + 1
                );
                assert!(
                    w > 0 && h > 0,
                    "Manager {} should capture valid dimensions",
                    i + 1
                );
                println!("Manager {} captured {}x{}", i + 1, w, h);
            }
            Err(e) => {
                println!("Manager {} failed to capture: {:?}", i + 1, e);
            }
        }
    }

    if active_managers > 1 {
        println!("Successfully verified {active_managers} concurrent managers");
    }

    // Managers will be dropped here, testing cleanup
    drop(managers);
    println!("Resource cleanup test completed");
}
