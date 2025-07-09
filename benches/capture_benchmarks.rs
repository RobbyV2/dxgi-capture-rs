use criterion::{Criterion, black_box, criterion_group, criterion_main};
use dxgi_capture_rs::DXGIManager;

fn bench_capture_frame(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("capture_frame", |b| {
        b.iter(|| {
            let result = manager.capture_frame();
            black_box(result)
        })
    });
}

fn bench_capture_frame_components(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("capture_frame_components", |b| {
        b.iter(|| {
            let result = manager.capture_frame_components();
            black_box(result)
        })
    });
}

fn bench_geometry(c: &mut Criterion) {
    let manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("geometry", |b| {
        b.iter(|| {
            let geometry = manager.geometry();
            black_box(geometry)
        })
    });
}

fn bench_manager_creation(c: &mut Criterion) {
    c.bench_function("manager_creation", |b| {
        b.iter(|| {
            let result = DXGIManager::new(1000);
            black_box(result)
        })
    });
}

fn bench_timeout_operations(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("timeout_operations", |b| {
        b.iter(|| {
            manager.set_timeout_ms(500);
            let timeout = manager.get_timeout_ms();
            black_box(timeout)
        })
    });
}

fn bench_capture_source_operations(c: &mut Criterion) {
    let manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("capture_source_operations", |b| {
        b.iter(|| {
            let index = manager.get_capture_source_index();
            black_box(index)
        })
    });
}

fn bench_capture_source_setting(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("capture_source_setting", |b| {
        b.iter(|| {
            manager.set_capture_source_index(0);
            black_box(())
        })
    });
}

fn bench_capture_frame_fast(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(100) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("capture_frame_fast", |b| {
        b.iter(|| {
            let result = manager.capture_frame_fast();
            black_box(result)
        })
    });
}

fn bench_capture_performance_regression(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(100) {
        Ok(m) => m,
        Err(_) => return,
    };

    let _ = manager.capture_frame_fast();

    c.bench_function("capture_performance_regression", |b| {
        b.iter(|| {
            let result = manager.capture_frame_fast();
            black_box(result)
        })
    });
}

fn bench_memory_efficiency(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    let (width, height) = manager.geometry();
    let expected_pixels = width * height;

    c.bench_function("memory_efficiency", |b| {
        b.iter(|| {
            let result = manager.capture_frame();
            if let Ok((pixels, (w, h))) = &result {
                assert_eq!(pixels.len(), expected_pixels);
                assert_eq!(w * h, expected_pixels);
            }
            black_box(result)
        })
    });
}

fn bench_capture_frame_with_metadata(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("capture_frame_with_metadata", |b| {
        b.iter(|| {
            let result = manager.capture_frame_with_metadata();
            black_box(result)
        })
    });
}

fn bench_capture_frame_components_with_metadata(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("capture_frame_components_with_metadata", |b| {
        b.iter(|| {
            let result = manager.capture_frame_components_with_metadata();
            black_box(result)
        })
    });
}

fn bench_metadata_processing(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    c.bench_function("metadata_processing", |b| {
        b.iter(|| {
            let result = manager.capture_frame_with_metadata();
            if let Ok((_, _, metadata)) = &result {
                // Simulate processing metadata
                let has_updates = metadata.has_updates();
                let has_mouse = metadata.has_mouse_updates();
                let change_count = metadata.total_change_count();
                black_box((has_updates, has_mouse, change_count));
            }
            black_box(result)
        })
    });
}

fn bench_metadata_vs_regular_capture(c: &mut Criterion) {
    let mut manager = match DXGIManager::new(1000) {
        Ok(m) => m,
        Err(_) => return,
    };

    let mut group = c.benchmark_group("metadata_vs_regular");

    group.bench_function("regular_capture", |b| {
        b.iter(|| {
            let result = manager.capture_frame();
            black_box(result)
        })
    });

    group.bench_function("metadata_capture", |b| {
        b.iter(|| {
            let result = manager.capture_frame_with_metadata();
            black_box(result)
        })
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_capture_frame,
    bench_capture_frame_components,
    bench_capture_frame_fast,
    bench_geometry,
    bench_manager_creation,
    bench_timeout_operations,
    bench_capture_source_operations,
    bench_capture_source_setting,
    bench_capture_performance_regression,
    bench_memory_efficiency,
    bench_capture_frame_with_metadata,
    bench_capture_frame_components_with_metadata,
    bench_metadata_processing,
    bench_metadata_vs_regular_capture
);

criterion_main!(benches);
