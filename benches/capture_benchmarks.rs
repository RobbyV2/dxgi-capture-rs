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
    bench_memory_efficiency
);

criterion_main!(benches);
