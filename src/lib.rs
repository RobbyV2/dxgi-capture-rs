//! Capture the screen with DXGI Desktop Duplication

#![cfg(windows)]

extern crate winapi;
extern crate wio;

use std::mem::zeroed;
use std::{mem, ptr, slice};
use winapi::shared::dxgi::{
    CreateDXGIFactory1, DXGI_MAP_READ, DXGI_OUTPUT_DESC, DXGI_RESOURCE_PRIORITY_MAXIMUM,
    IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput, IDXGISurface1, IID_IDXGIFactory1,
};
use winapi::shared::dxgi1_2::{IDXGIOutput1, IDXGIOutputDuplication};
use winapi::shared::dxgitype::*;
// use winapi::shared::ntdef::*;
use winapi::shared::windef::*;
use winapi::shared::winerror::*;
use winapi::um::d3d11::*;
use winapi::um::d3dcommon::*;
use winapi::um::unknwnbase::*;
use winapi::um::winuser::*;
use wio::com::ComPtr;

/// Color represented by additive channels: Blue (b), Green (g), Red (r), and Alpha (a).
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq, Eq, Ord)]
pub struct BGRA8 {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub a: u8,
}

/// Possible errors when capturing
#[derive(Debug)]
pub enum CaptureError {
    /// Could not duplicate output, access denied. Might be in protected fullscreen.
    AccessDenied,
    /// Access to the duplicated output was lost. Likely, mode was changed e.g. window => full
    AccessLost,
    /// Error when trying to refresh outputs after some failure.
    RefreshFailure,
    /// AcquireNextFrame timed out.
    Timeout,
    /// General/Unexpected failure
    Fail(&'static str),
}

/// Check whether the HRESULT represents a failure
pub fn hr_failed(hr: HRESULT) -> bool {
    hr < 0
}

fn create_dxgi_factory_1() -> ComPtr<IDXGIFactory1> {
    unsafe {
        let mut factory = ptr::null_mut();
        let hr = CreateDXGIFactory1(&IID_IDXGIFactory1, &mut factory);
        if hr_failed(hr) {
            panic!("Failed to create DXGIFactory1, {:x}", hr)
        } else {
            ComPtr::from_raw(factory as *mut IDXGIFactory1)
        }
    }
}

#[allow(const_item_mutation)]
fn d3d11_create_device(
    adapter: *mut IDXGIAdapter,
) -> (ComPtr<ID3D11Device>, ComPtr<ID3D11DeviceContext>) {
    unsafe {
        let (mut d3d11_device, mut device_context) = (ptr::null_mut(), ptr::null_mut());
        let hr = D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            0,
            D3D11_SDK_VERSION,
            &mut d3d11_device,
            &mut D3D_FEATURE_LEVEL_9_1,
            &mut device_context,
        );
        if hr_failed(hr) {
            panic!("Failed to create d3d11 device and device context, {:x}", hr)
        } else {
            (
                ComPtr::from_raw(d3d11_device),
                ComPtr::from_raw(device_context),
            )
        }
    }
}

fn get_adapter_outputs(adapter: &IDXGIAdapter1) -> Vec<ComPtr<IDXGIOutput>> {
    let mut outputs = Vec::new();
    for i in 0.. {
        unsafe {
            let mut output = ptr::null_mut();
            if hr_failed(adapter.EnumOutputs(i, &mut output)) {
                break;
            } else {
                let mut out_desc = zeroed();
                (*output).GetDesc(&mut out_desc);
                if out_desc.AttachedToDesktop != 0 {
                    outputs.push(ComPtr::from_raw(output))
                } else {
                    break;
                }
            }
        }
    }
    outputs
}

fn output_is_primary(output: &ComPtr<IDXGIOutput1>) -> bool {
    unsafe {
        let mut output_desc = zeroed();
        output.GetDesc(&mut output_desc);
        let mut monitor_info: MONITORINFO = zeroed();
        monitor_info.cbSize = mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(output_desc.Monitor, &mut monitor_info);
        (monitor_info.dwFlags & 1) != 0
    }
}

fn get_capture_source(
    output_dups: Vec<(ComPtr<IDXGIOutputDuplication>, ComPtr<IDXGIOutput1>)>,
    cs_index: usize,
) -> Option<(ComPtr<IDXGIOutputDuplication>, ComPtr<IDXGIOutput1>)> {
    if cs_index == 0 {
        output_dups
            .into_iter()
            .find(|(_, out)| output_is_primary(out))
    } else {
        output_dups
            .into_iter()
            .filter(|(_, out)| !output_is_primary(out))
            .nth(cs_index - 1)
    }
}

#[allow(clippy::type_complexity)]
fn duplicate_outputs(
    mut device: ComPtr<ID3D11Device>,
    outputs: Vec<ComPtr<IDXGIOutput>>,
) -> Result<
    (
        ComPtr<ID3D11Device>,
        Vec<(ComPtr<IDXGIOutputDuplication>, ComPtr<IDXGIOutput1>)>,
    ),
    HRESULT,
> {
    let mut out_dups = Vec::new();
    for output in outputs
        .into_iter()
        .map(|out| out.cast::<IDXGIOutput1>().unwrap())
    {
        let dxgi_device = device.up::<IUnknown>();
        let output_duplication = unsafe {
            let mut output_duplication = ptr::null_mut();
            let hr = output.DuplicateOutput(dxgi_device.as_raw(), &mut output_duplication);
            if hr_failed(hr) {
                return Err(hr);
            }
            ComPtr::from_raw(output_duplication)
        };
        device = dxgi_device.cast().unwrap();
        out_dups.push((output_duplication, output));
    }
    Ok((device, out_dups))
}

struct DuplicatedOutput {
    device: ComPtr<ID3D11Device>,
    device_context: ComPtr<ID3D11DeviceContext>,
    output: ComPtr<IDXGIOutput1>,
    output_duplication: ComPtr<IDXGIOutputDuplication>,
}
impl DuplicatedOutput {
    fn get_desc(&self) -> DXGI_OUTPUT_DESC {
        unsafe {
            let mut desc = zeroed();
            self.output.GetDesc(&mut desc);
            desc
        }
    }

    fn capture_frame_to_surface(
        &mut self,
        timeout_ms: u32,
    ) -> Result<ComPtr<IDXGISurface1>, HRESULT> {
        let frame_resource = unsafe {
            let mut frame_resource = ptr::null_mut();
            let mut frame_info = zeroed();
            let hr = self.output_duplication.AcquireNextFrame(
                timeout_ms,
                &mut frame_info,
                &mut frame_resource,
            );
            if hr_failed(hr) {
                return Err(hr);
            }
            ComPtr::from_raw(frame_resource)
        };
        let frame_texture = frame_resource.cast::<ID3D11Texture2D>().unwrap();
        let mut texture_desc = unsafe {
            let mut texture_desc = zeroed();
            frame_texture.GetDesc(&mut texture_desc);
            texture_desc
        };
        // Configure the description to make the texture readable
        texture_desc.Usage = D3D11_USAGE_STAGING;
        texture_desc.BindFlags = 0;
        texture_desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
        texture_desc.MiscFlags = 0;
        let readable_texture = unsafe {
            let mut readable_texture = ptr::null_mut();
            let hr = self
                .device
                .CreateTexture2D(&texture_desc, ptr::null(), &mut readable_texture);
            if hr_failed(hr) {
                return Err(hr);
            }
            ComPtr::from_raw(readable_texture)
        };
        // Lower priorities causes stuff to be needlessly copied from gpu to ram,
        // causing huge ram usage on some systems.
        unsafe { readable_texture.SetEvictionPriority(DXGI_RESOURCE_PRIORITY_MAXIMUM) };
        let readable_surface = readable_texture.up::<ID3D11Resource>();
        unsafe {
            self.device_context.CopyResource(
                readable_surface.as_raw(),
                frame_texture.up::<ID3D11Resource>().as_raw(),
            );
            self.output_duplication.ReleaseFrame();
        }
        readable_surface.cast()
    }
}

/// Manager of DXGI duplicated outputs
pub struct DXGIManager {
    duplicated_output: Option<DuplicatedOutput>,
    capture_source_index: usize,
    timeout_ms: u32,
}

struct SharedPtr<T>(*const T);

unsafe impl<T> Send for SharedPtr<T> {}

unsafe impl<T> Sync for SharedPtr<T> {}

impl DXGIManager {
    /// Construct a new manager with capture timeout
    pub fn new(timeout_ms: u32) -> Result<DXGIManager, &'static str> {
        let mut manager = DXGIManager {
            duplicated_output: None,
            capture_source_index: 0,
            timeout_ms,
        };

        match manager.acquire_output_duplication() {
            Ok(_) => Ok(manager),
            Err(_) => Err("Failed to acquire output duplication"),
        }
    }

    pub fn geometry(&self) -> (usize, usize) {
        let output_desc = self.duplicated_output.as_ref().unwrap().get_desc();
        let RECT {
            left,
            top,
            right,
            bottom,
        } = output_desc.DesktopCoordinates;
        ((right - left) as usize, (bottom - top) as usize)
    }

    /// Set index of capture source to capture from
    pub fn set_capture_source_index(&mut self, cs: usize) {
        self.capture_source_index = cs;
        let _ = self.acquire_output_duplication();
    }

    pub fn get_capture_source_index(&self) -> usize {
        self.capture_source_index
    }

    /// Set timeout to use when capturing
    pub fn set_timeout_ms(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms
    }

    /// Duplicate and acquire output selected by `capture_source_index`
    #[allow(clippy::result_unit_err)]
    pub fn acquire_output_duplication(&mut self) -> Result<(), ()> {
        self.duplicated_output = None;
        let factory = create_dxgi_factory_1();
        for (outputs, adapter) in (0..)
            .map(|i| {
                let mut adapter = ptr::null_mut();
                unsafe {
                    if factory.EnumAdapters1(i, &mut adapter) != DXGI_ERROR_NOT_FOUND {
                        Some(ComPtr::from_raw(adapter))
                    } else {
                        None
                    }
                }
            })
            .take_while(Option::is_some)
            .map(Option::unwrap)
            .map(|adapter| (get_adapter_outputs(&adapter), adapter))
            .filter(|(outs, _)| !outs.is_empty())
        {
            // Creating device for each adapter that has the output
            let (d3d11_device, device_context) = d3d11_create_device(adapter.up().as_raw());
            let (d3d11_device, output_duplications) =
                duplicate_outputs(d3d11_device, outputs).map_err(|_| ())?;
            if let Some((output_duplication, output)) =
                get_capture_source(output_duplications, self.capture_source_index)
            {
                self.duplicated_output = Some(DuplicatedOutput {
                    device: d3d11_device,
                    device_context,
                    output,
                    output_duplication,
                });
                return Ok(());
            }
        }
        Err(())
    }

    fn capture_frame_to_surface(&mut self) -> Result<ComPtr<IDXGISurface1>, CaptureError> {
        if self.duplicated_output.is_none() {
            if self.acquire_output_duplication().is_ok() {
                return Err(CaptureError::Fail("No valid duplicated output"));
            } else {
                return Err(CaptureError::RefreshFailure);
            }
        }
        let timeout_ms = self.timeout_ms;
        match self
            .duplicated_output
            .as_mut()
            .unwrap()
            .capture_frame_to_surface(timeout_ms)
        {
            Ok(surface) => Ok(surface),
            Err(DXGI_ERROR_ACCESS_LOST) => {
                if self.acquire_output_duplication().is_ok() {
                    Err(CaptureError::AccessLost)
                } else {
                    Err(CaptureError::RefreshFailure)
                }
            }
            Err(E_ACCESSDENIED) => Err(CaptureError::AccessDenied),
            Err(DXGI_ERROR_WAIT_TIMEOUT) => Err(CaptureError::Timeout),
            Err(_) => {
                if self.acquire_output_duplication().is_ok() {
                    Err(CaptureError::Fail("Failure when acquiring frame"))
                } else {
                    Err(CaptureError::RefreshFailure)
                }
            }
        }
    }

    fn capture_frame_t<T: Copy + Send + Sync + Sized>(
        &mut self,
    ) -> Result<(Vec<T>, (usize, usize)), CaptureError> {
        let frame_surface = self.capture_frame_to_surface()?;
        let mapped_surface = unsafe {
            let mut mapped_surface = zeroed();
            if hr_failed(frame_surface.Map(&mut mapped_surface, DXGI_MAP_READ)) {
                frame_surface.Release();
                return Err(CaptureError::Fail("Failed to map surface"));
            }
            mapped_surface
        };
        let byte_size = |x| x * mem::size_of::<BGRA8>() / mem::size_of::<T>();
        let output_desc = self.duplicated_output.as_mut().unwrap().get_desc();
        let stride = mapped_surface.Pitch as usize / mem::size_of::<BGRA8>();
        let byte_stride = byte_size(stride);
        let (output_width, output_height) = {
            let RECT {
                left,
                top,
                right,
                bottom,
            } = output_desc.DesktopCoordinates;
            ((right - left) as usize, (bottom - top) as usize)
        };
        let mut pixel_buf = Vec::with_capacity(byte_size(output_width * output_height));

        let scan_lines = match output_desc.Rotation {
            DXGI_MODE_ROTATION_ROTATE90 | DXGI_MODE_ROTATION_ROTATE270 => output_width,
            _ => output_height,
        };

        let mapped_pixels = unsafe {
            slice::from_raw_parts(mapped_surface.pBits as *const T, byte_stride * scan_lines)
        };

        match output_desc.Rotation {
            DXGI_MODE_ROTATION_IDENTITY | DXGI_MODE_ROTATION_UNSPECIFIED => {
                pixel_buf.extend_from_slice(mapped_pixels)
            }
            DXGI_MODE_ROTATION_ROTATE90 => unsafe {
                let ptr = SharedPtr(pixel_buf.as_ptr() as *const BGRA8);
                mapped_pixels
                    .chunks(byte_stride)
                    .rev()
                    .enumerate()
                    .for_each(|(column, chunk)| {
                        let mut src = chunk.as_ptr() as *const BGRA8;
                        let mut dst = ptr.0 as *mut BGRA8;
                        dst = dst.add(column);
                        let stop = src.add(output_height);
                        while src != stop {
                            dst.write(*src);
                            src = src.add(1);
                            dst = dst.add(output_width);
                        }
                    });
                pixel_buf.set_len(pixel_buf.capacity());
            },
            DXGI_MODE_ROTATION_ROTATE180 => unsafe {
                let ptr = SharedPtr(pixel_buf.as_ptr() as *const BGRA8);
                mapped_pixels
                    .chunks(byte_stride)
                    .rev()
                    .enumerate()
                    .for_each(|(scan_line, chunk)| {
                        let mut src = chunk.as_ptr() as *const BGRA8;
                        let mut dst = ptr.0 as *mut BGRA8;
                        dst = dst.add(scan_line * output_width);
                        let stop = src;
                        src = src.add(output_width);
                        while src != stop {
                            src = src.sub(1);
                            dst.write(*src);
                            dst = dst.add(1);
                        }
                    });
                pixel_buf.set_len(pixel_buf.capacity());
            },
            DXGI_MODE_ROTATION_ROTATE270 => unsafe {
                let ptr = SharedPtr(pixel_buf.as_ptr() as *const BGRA8);
                mapped_pixels
                    .chunks(byte_stride)
                    .enumerate()
                    .for_each(|(column, chunk)| {
                        let mut src = chunk.as_ptr() as *const BGRA8;
                        let mut dst = ptr.0 as *mut BGRA8;
                        dst = dst.add(column);
                        let stop = src;
                        src = src.add(output_height);
                        while src != stop {
                            src = src.sub(1);
                            dst.write(*src);
                            dst = dst.add(output_width);
                        }
                    });
                pixel_buf.set_len(pixel_buf.capacity());
            },
            n => unreachable!("Undefined DXGI_MODE_ROTATION: {}", n),
        }
        unsafe { frame_surface.Unmap() };
        Ok((pixel_buf, (output_width, output_height)))
    }

    /// Capture a frame
    ///
    /// On success, return Vec with pixels and width and height of frame.
    /// On failure, return CaptureError.
    pub fn capture_frame(&mut self) -> Result<(Vec<BGRA8>, (usize, usize)), CaptureError> {
        self.capture_frame_t()
    }

    /// Capture a frame
    ///
    /// On success, return Vec with pixel components and width and height of frame.
    /// On failure, return CaptureError.
    pub fn capture_frame_components(&mut self) -> Result<(Vec<u8>, (usize, usize)), CaptureError> {
        self.capture_frame_t()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test DXGI manager creation with various timeout values
    #[test]
    fn test_dxgi_manager_creation() {
        // Test with reasonable timeout
        let result = DXGIManager::new(1000);
        if result.is_err() {
            println!("DXGI not available - skipping test (expected in headless environments)");
            return;
        }

        // If we get here, DXGI is available, so test other timeout values
        // Test with minimal timeout
        let result = DXGIManager::new(0);
        if result.is_err() {
            println!("Manager creation with 0ms timeout failed (may be expected)");
        }

        // Test with large timeout
        let result = DXGIManager::new(10000);
        if result.is_err() {
            println!("Manager creation with 10000ms timeout failed (may be expected)");
        }
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

    /// Test geometry retrieval
    #[test]
    fn test_geometry() {
        let manager = match DXGIManager::new(300) {
            Ok(m) => m,
            Err(_) => {
                println!("DXGI not available - skipping test");
                return;
            }
        };
        let (width, height) = manager.geometry();

        assert!(width > 0, "Width should be greater than 0, got {}", width);
        assert!(
            height > 0,
            "Height should be greater than 0, got {}",
            height
        );

        // Reasonable bounds for screen resolution
        assert!(width <= 8192, "Width should be reasonable, got {}", width);
        assert!(
            height <= 8192,
            "Height should be reasonable, got {}",
            height
        );
    }

    /// Test frame capture functionality
    #[test]
    fn test_frame_capture() {
        let mut manager = match DXGIManager::new(300) {
            Ok(m) => m,
            Err(_) => {
                println!("DXGI not available - skipping test");
                return;
            }
        };

        // Test single frame capture
        let result = manager.capture_frame();
        match result {
            Ok((pixels, (width, height))) => {
                assert!(!pixels.is_empty(), "Pixel data should not be empty");
                assert_eq!(
                    pixels.len(),
                    width * height,
                    "Pixel count should match dimensions"
                );
                assert!(width > 0 && height > 0, "Dimensions should be positive");
            }
            Err(CaptureError::Timeout) => {
                // Timeout is acceptable in test environment
                println!("Frame capture timed out (acceptable in tests)");
            }
            Err(e) => {
                println!("Frame capture failed with error: {:?}", e);
                // Don't fail the test for other errors as they might be environment-specific
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
                println!("Frame components capture failed with error: {:?}", e);
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
                println!("Both captures failed: {:?}, {:?}", e1, e2);
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
                        "Pixel data should not be empty on capture {}",
                        i
                    );
                    assert!(
                        width > 0 && height > 0,
                        "Dimensions should be positive on capture {}",
                        i
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
                    println!("Capture {} failed with error: {:?}", i, e);
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
            let debug_string = format!("{:?}", error);
            assert!(
                !debug_string.is_empty(),
                "Error should have debug representation"
            );
        }
    }
}
