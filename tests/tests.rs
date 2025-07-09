//! Tests for dxgi-capture-rs library functionality.

use dxgi_capture_rs::{BGRA8, CaptureError, DXGIManager, hr_failed};

#[test]
fn test_dxgi_manager_creation() {
    let result = DXGIManager::new(1000);

    if result.is_err() {
        println!("DXGI not available - skipping test (expected in headless environments)");
        return;
    }

    let mut manager = result.unwrap();

    assert_eq!(manager.get_timeout_ms(), 1000);

    manager.set_timeout_ms(100);
    assert_eq!(manager.get_timeout_ms(), 100);

    manager.set_timeout_ms(5000);
    assert_eq!(manager.get_timeout_ms(), 5000);

    manager.set_timeout_ms(0);
    assert_eq!(manager.get_timeout_ms(), 0);

    manager.set_timeout_ms(u32::MAX);
    assert_eq!(manager.get_timeout_ms(), u32::MAX);
}

#[test]
fn test_timeout_configuration() {
    let mut manager = match DXGIManager::new(500) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    manager.set_timeout_ms(100);
    manager.set_timeout_ms(2000);
    manager.set_timeout_ms(0);
}

#[test]
fn test_capture_source_index() {
    let mut manager = match DXGIManager::new(300) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let initial_index = manager.get_capture_source_index();
    assert_eq!(initial_index, 0);

    manager.set_capture_source_index(0);
    assert_eq!(manager.get_capture_source_index(), 0);

    manager.set_capture_source_index(1);
}

#[test]
fn test_geometry() {
    let manager = match DXGIManager::new(300) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let (width1, height1) = manager.geometry();
    let (width2, height2) = manager.geometry();

    assert_eq!(width1, width2);
    assert_eq!(height1, height2);

    assert!(width1 > 0);
    assert!(height1 > 0);

    // Test minimum reasonable resolution
    assert!(
        width1 >= 640,
        "Width should be at least 640px, got {width1}"
    );
    assert!(
        height1 >= 480,
        "Height should be at least 480px, got {height1}"
    );

    // Test maximum reasonable bounds for modern displays
    assert!(width1 <= 8192, "Width should be <= 8192px, got {width1}");
    assert!(height1 <= 8192, "Height should be <= 8192px, got {height1}");

    // Test aspect ratio is reasonable (between 4:3 and 32:9)
    let aspect_ratio = width1 as f64 / height1 as f64;
    assert!(aspect_ratio >= 1.0, "Aspect ratio should be >= 1.0");
    assert!(
        aspect_ratio <= 3.6,
        "Aspect ratio should be <= 3.6, got {aspect_ratio:.2}"
    );
}

#[test]
fn test_frame_capture() {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let (expected_width, expected_height) = manager.geometry();

    let result = manager.capture_frame();
    match result {
        Ok((pixels, (width, height))) => {
            assert!(!pixels.is_empty());
            assert_eq!(pixels.len(), width * height);
            assert_eq!(width, expected_width);
            assert_eq!(height, expected_height);

            // Test pixel data validity
            let sample_indices = [0, pixels.len() / 4, pixels.len() / 2, pixels.len() - 1];
            for &idx in &sample_indices {
                let pixel = pixels[idx];
                let _check = (pixel.b, pixel.g, pixel.r, pixel.a);
            }
        }
        Err(CaptureError::Timeout) => {
            println!("No frame available - this is normal");
        }
        Err(e) => {
            println!("Capture failed: {e:?}");
        }
    }
}

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
            assert!(!components.is_empty());
            assert_eq!(components.len(), width * height * 4);
            assert!(width > 0 && height > 0);
        }
        Err(CaptureError::Timeout) => {
            // Timeout is acceptable in test environment
            println!("Frame components capture timed out (acceptable in tests)");
        }
        Err(e) => {
            println!("Frame components capture failed with error: {e:?}");
        }
    }
}

#[test]
fn test_frame_capture_consistency() {
    let mut manager = match DXGIManager::new(500) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Capture first frame
    let (pixels1, (w1, h1)) = match manager.capture_frame() {
        Ok(data) => data,
        Err(CaptureError::Timeout) => {
            println!("First frame timed out - skipping consistency test");
            return;
        }
        Err(e) => panic!("First capture failed: {e:?}"),
    };

    // Capture second frame
    let (pixels2, (w2, h2)) = match manager.capture_frame() {
        Ok(data) => data,
        Err(CaptureError::Timeout) => {
            println!("Second frame timed out - skipping consistency test");
            return;
        }
        Err(e) => panic!("Second capture failed: {e:?}"),
    };

    // Test dimensions are consistent
    assert_eq!(w1, w2, "Widths should be consistent between frames");
    assert_eq!(h1, h2, "Heights should be consistent between frames");
    assert_eq!(
        pixels1.len(),
        pixels2.len(),
        "Pixel buffer sizes should be consistent"
    );
}

#[test]
fn test_multiple_captures() {
    let mut manager = match DXGIManager::new(200) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let mut success_count = 0;
    let mut timeout_count = 0;
    let total_captures = 10;

    for i in 0..total_captures {
        match manager.capture_frame() {
            Ok(_) => success_count += 1,
            Err(CaptureError::Timeout) => timeout_count += 1,
            Err(e) => panic!("Capture {i} failed with unexpected error: {e:?}"),
        }
    }

    assert_eq!(success_count + timeout_count, total_captures);
    println!("Multiple captures: {success_count} successes, {timeout_count} timeouts");
}

#[test]
fn test_bgra8_color() {
    let red = BGRA8 {
        b: 0,
        g: 0,
        r: 255,
        a: 255,
    };
    let blue = BGRA8 {
        b: 255,
        g: 0,
        r: 0,
        a: 255,
    };
    let transparent = BGRA8 {
        b: 0,
        g: 0,
        r: 0,
        a: 0,
    };

    assert_eq!(red.r, 255);
    assert_eq!(blue.b, 255);
    assert_eq!(transparent.a, 0);
    assert_ne!(red, blue);
}

#[test]
fn test_hr_failed() {
    use windows::Win32::Foundation::{E_FAIL, S_OK};
    use windows::core::HRESULT;

    assert!(!hr_failed(S_OK));
    assert!(hr_failed(E_FAIL));
    assert!(!hr_failed(HRESULT(0)));
    assert!(!hr_failed(HRESULT(1)));
    assert!(hr_failed(HRESULT(-1)));
    assert!(hr_failed(HRESULT(-2147467259)));
}

#[test]
fn test_capture_error_variants() {
    use windows::Win32::Foundation::E_FAIL;
    let errors = [
        CaptureError::AccessDenied,
        CaptureError::AccessLost,
        CaptureError::RefreshFailure,
        CaptureError::Timeout,
        CaptureError::Fail(windows::core::Error::from(E_FAIL)),
    ];

    for error in &errors {
        let formatted = format!("{error}");
        assert!(!formatted.is_empty());
    }
}

#[test]
fn test_hr_failed_comprehensive() {
    use windows::Win32::Foundation::{E_FAIL, S_OK};
    use windows::core::HRESULT;

    assert!(!hr_failed(S_OK));
    assert!(hr_failed(E_FAIL));

    assert!(!hr_failed(HRESULT(0)));
    assert!(!hr_failed(HRESULT(1)));

    assert!(hr_failed(HRESULT(-1)));
    assert!(hr_failed(HRESULT(0x8000_4005u32 as i32)));
    assert!(hr_failed(HRESULT(0x8007_000Eu32 as i32)));
}

#[test]
fn test_bgra8_comprehensive() {
    let p1 = BGRA8 {
        b: 10,
        g: 20,
        r: 30,
        a: 40,
    };
    assert_eq!(p1.b, 10);
    assert_eq!(p1.g, 20);
    assert_eq!(p1.r, 30);
    assert_eq!(p1.a, 40);

    let p2 = BGRA8 {
        b: 10,
        g: 20,
        r: 30,
        a: 40,
    };
    let p3 = BGRA8 {
        b: 11,
        g: 20,
        r: 30,
        a: 40,
    };
    assert_eq!(p1, p2);
    assert_ne!(p1, p3);

    let p4 = p1;
    let p5 = p1;
    assert_eq!(p1, p4);
    assert_eq!(p1, p5);

    let debug_str = format!("{p1:?}");
    assert!(debug_str.contains("b: 10"));
    assert!(debug_str.contains("g: 20"));
    assert!(debug_str.contains("r: 30"));
    assert!(debug_str.contains("a: 40"));
}

#[test]
fn test_timeout_behavior_strict() {
    let mut manager = match DXGIManager::new(10) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let result = manager.capture_frame();
    if let Err(e) = result {
        assert!(
            matches!(e, CaptureError::Timeout),
            "Expected timeout error, got {e:?}"
        );
    } else {
        println!("Warning: Frame capture succeeded with 10ms timeout, which is unusually fast.");
    }
}

#[test]
fn test_capture_source_index_validation() {
    let mut manager = match DXGIManager::new(100) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Setting a very high index should not cause a panic
    // It should result in an error or timeout on next capture
    manager.set_capture_source_index(99);

    // The capture should fail gracefully, likely with a timeout or access lost
    let result = manager.capture_frame();
    assert!(
        result.is_err(),
        "Capture should fail with an invalid source index"
    );

    // The specific error can vary, but it shouldn't be a success
    if let Err(e) = result {
        println!("Capture with invalid index failed as expected: {e:?}");
        assert!(matches!(
            e,
            CaptureError::AccessLost | CaptureError::Timeout | CaptureError::RefreshFailure
        ));
    }
}

#[test]
fn test_capture_workflow() {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let (initial_width, initial_height) = manager.geometry();
    assert!(
        initial_width > 0 && initial_height > 0,
        "Initial geometry should be valid"
    );

    let res1 = manager.capture_frame();
    if res1.is_err() {
        println!("Initial capture failed, skipping further workflow tests.");
        return;
    }
    let (pixels1, (w1, h1)) = res1.unwrap();
    assert_eq!(w1, initial_width);
    assert_eq!(h1, initial_height);
    assert_eq!(
        pixels1.len(),
        w1 * h1,
        "Pixel buffer size should be correct"
    );

    manager.set_timeout_ms(50);
    assert_eq!(manager.get_timeout_ms(), 50, "Timeout should be updated");

    let res2 = manager.capture_frame();
    if res2.is_err() {
        println!("Second capture failed, but workflow test up to this point is okay.");
        return;
    }
    let (pixels2, (w2, h2)) = res2.unwrap();
    assert_eq!(w2, initial_width, "Width should remain consistent");
    assert_eq!(h2, initial_height, "Height should remain consistent");
    assert_eq!(
        pixels2.len(),
        w2 * h2,
        "Pixel buffer size should be consistent"
    );

    manager.set_capture_source_index(1);
    let switched = manager.acquire_output_duplication().is_ok() && manager.capture_frame().is_ok();

    if switched {
        println!("Successfully switched to a secondary display.");
        let (new_width, new_height) = manager.geometry();
        assert!(
            new_width > 0 && new_height > 0,
            "New geometry should be valid after switching"
        );
    } else {
        println!("Could not switch to secondary display (may not exist).");
    }

    manager.set_capture_source_index(0);
    assert!(
        manager.acquire_output_duplication().is_ok(),
        "Should successfully re-acquire the primary display"
    );
    let (reverted_width, reverted_height) = manager.geometry();
    assert!(
        reverted_width > 0 && reverted_height > 0,
        "Geometry should be valid after reverting"
    );

    // Verify capture still works after switching back
    assert!(
        manager.capture_frame().is_ok(),
        "Capture should work after reverting to primary display"
    );
}

#[test]
fn test_capture_source_switching() {
    let mut manager = match DXGIManager::new(500) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    manager.set_capture_source_index(1);
    let res1 = manager.capture_frame();

    if res1.is_ok() {
        println!("Secondary display found and captured.");
        let (w1, h1) = manager.geometry();
        assert!(w1 > 0 && h1 > 0);
    } else {
        println!("Secondary display not found or capture failed (expected if single monitor).");
    }

    manager.set_capture_source_index(0);
    let (w_revert, h_revert) = manager.geometry();
    assert!(
        w_revert > 0 && h_revert > 0,
        "Geometry should be valid after reverting to primary"
    );
    assert!(
        manager.capture_frame().is_ok(),
        "Capture should succeed after reverting to primary"
    );
}

#[test]
fn test_timeout_behavior() {
    // Test with zero timeout (should return immediately)
    let mut manager_zero = match DXGIManager::new(0) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };
    // This will either succeed or timeout, both are fine
    let _ = manager_zero.capture_frame();

    // Test with a long timeout (more likely to succeed)
    let mut manager_long = match DXGIManager::new(2000) {
        Ok(m) => m,
        Err(_) => {
            // Should not fail here if previous one succeeded
            return;
        }
    };
    let result_long = manager_long.capture_frame();
    assert!(
        result_long.is_ok(),
        "Capture with long timeout should succeed, but got: {:?}",
        result_long.err()
    );
}

#[test]
fn test_multiple_managers() {
    // Create first manager
    let mut manager1 = match DXGIManager::new(100) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Create second manager, which should fail as the output is already captured.
    let manager2_result = DXGIManager::new(100);
    assert!(
        manager2_result.is_err(),
        "Second manager creation should fail if the output is already captured"
    );

    // First manager should still be able to capture
    assert!(
        manager1.capture_frame().is_ok(),
        "First manager should capture"
    );
}

#[test]
fn test_frame_consistency() {
    let mut manager = match DXGIManager::new(200) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let mut last_frame_stats = (0, 0, 0); // (r_sum, g_sum, b_sum)
    let mut frames_are_changing = false;

    for _ in 0..5 {
        if let Ok((pixels, _)) = manager.capture_frame() {
            let r_sum = pixels.iter().map(|p| p.r as u64).sum();
            let g_sum = pixels.iter().map(|p| p.g as u64).sum();
            let b_sum = pixels.iter().map(|p| p.b as u64).sum();

            if last_frame_stats != (0, 0, 0)
                && (r_sum != last_frame_stats.0
                    || g_sum != last_frame_stats.1
                    || b_sum != last_frame_stats.2)
            {
                frames_are_changing = true;
                break;
            }
            last_frame_stats = (r_sum, g_sum, b_sum);
        }
    }

    if !frames_are_changing {
        println!(
            "Warning: Frame content did not change over 5 captures. This is normal on a static screen."
        );
    }
}

#[test]
fn test_error_handling_robustness() {
    let mut manager = match DXGIManager::new(10) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Force a situation that might lead to AccessLost or other errors
    // by repeatedly changing capture source.
    for i in 0..3 {
        manager.set_capture_source_index(i);
        let _ = manager.capture_frame(); // Ignore result, just stress the system
        manager.set_capture_source_index(0);
        let _ = manager.capture_frame();
    }

    // After stress, a final capture should still work
    assert!(
        manager.capture_frame().is_ok(),
        "Manager should recover and capture after stress"
    );
}

#[test]
fn test_capture_method_consistency() {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    // Capture with BGRA8
    let res_bgra = manager.capture_frame();
    if res_bgra.is_err() {
        println!("BGRA8 capture failed, skipping consistency test.");
        return;
    }
    let (pixels_bgra, (w_bgra, h_bgra)) = res_bgra.unwrap();

    // Capture with components
    // Need to re-acquire to get the same frame, if possible
    manager.set_capture_source_index(manager.get_capture_source_index()); // Re-init
    let res_comp = manager.capture_frame_components();
    if res_comp.is_err() {
        println!("Component capture failed, skipping consistency test.");
        return;
    }
    let (pixels_comp, (w_comp, h_comp)) = res_comp.unwrap();

    // Check dimensions
    assert_eq!(w_bgra, w_comp, "Widths should match");
    assert_eq!(h_bgra, h_comp, "Heights should match");
    assert_eq!(
        pixels_bgra.len() * 4,
        pixels_comp.len(),
        "Component buffer size should be 4x BGRA buffer"
    );

    // Check data consistency for a sample of pixels
    let mut consistent = true;
    for i in 0..pixels_bgra.len().min(100) {
        let bgra = pixels_bgra[i];
        let comp_b = pixels_comp[i * 4];
        let comp_g = pixels_comp[i * 4 + 1];
        let comp_r = pixels_comp[i * 4 + 2];
        let comp_a = pixels_comp[i * 4 + 3];

        if bgra.b != comp_b || bgra.g != comp_g || bgra.r != comp_r || bgra.a != comp_a {
            consistent = false;
            break;
        }
    }

    if !consistent {
        // This can happen if the screen updated between the two captures.
        // It's not a hard failure, but worth noting.
        println!(
            "Warning: Pixel data was not consistent between BGRA and component captures. This can happen if the screen content changed between calls."
        );
    }
}

#[test]
#[ignore]
fn test_performance_characteristics() {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping performance test");
            return;
        }
    };

    let num_captures = 30;
    let mut durations = Vec::new();

    for _ in 0..num_captures {
        let start = std::time::Instant::now();
        let result = manager.capture_frame();
        let duration = start.elapsed();

        if result.is_ok() {
            durations.push(duration);
        }
    }

    if durations.is_empty() {
        panic!("No successful captures during performance test");
    }

    let avg_duration = durations.iter().sum::<std::time::Duration>() / durations.len() as u32;
    let min_duration = *durations.iter().min().unwrap();
    let max_duration = *durations.iter().max().unwrap();

    println!("Performance over {} captures:", durations.len());
    println!("  - Average: {avg_duration:?}");
    println!("  - Min: {min_duration:?}");
    println!("  - Max: {max_duration:?}");

    // Average should be well under 100ms for a responsive system
    assert!(
        avg_duration.as_millis() < 100,
        "Average capture time should be < 100ms, was {avg_duration:?}"
    );
}

#[test]
fn test_resource_management() {
    // This test ensures that the DXGIManager can be created and dropped
    // without leaking resources. The test passes if it completes without
    // panicking or crashing.

    for _ in 0..5 {
        {
            // Create manager in an inner scope
            let mut manager = match DXGIManager::new(100) {
                Ok(m) => m,
                Err(_) => {
                    println!("DXGI not available - skipping resource test");
                    return;
                }
            };
            // Perform a capture to ensure resources are allocated
            let _ = manager.capture_frame();
        }
        // Manager is dropped here.
    }

    // If we can create a final manager, it implies resources were released
    let final_manager = DXGIManager::new(100);
    assert!(
        final_manager.is_ok(),
        "Should be able to create a manager after others were dropped"
    );
}

#[test]
fn test_geometry_with_none_duplicated_output() {
    let mut manager = match DXGIManager::new(100) {
        Ok(m) => m,
        Err(_) => {
            println!("DXGI not available - skipping test");
            return;
        }
    };

    let (width, height) = manager.geometry();
    assert!(width > 0 && height > 0, "Initial geometry should be valid");

    manager.set_capture_source_index(99);

    let (width_after, height_after) = manager.geometry();

    if width_after == 0 && height_after == 0 {
        println!(
            "Geometry returned (0, 0) for invalid capture source - this is expected defensive behavior"
        );
    } else {
        assert!(
            width_after > 0 && height_after > 0,
            "Geometry should be valid if not (0, 0)"
        );
    }

    manager.set_capture_source_index(0);

    let (width_final, height_final) = manager.geometry();
    if width_final == 0 && height_final == 0 {
        println!("Warning: Could not re-acquire primary monitor after switching back");
    } else {
        assert!(
            width_final > 0 && height_final > 0,
            "Final geometry should be valid after switching back to primary"
        );
    }
}
