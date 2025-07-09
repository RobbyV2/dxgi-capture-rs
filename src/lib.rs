//! High-performance screen capturing with DXGI Desktop Duplication API for Windows.
//!
//! This library provides a Rust interface to the Windows DXGI Desktop Duplication API,
//! enabling efficient screen capture with minimal performance overhead. It's designed
//! for applications that need real-time screen capture capabilities.
//!
//! # Features
//!
//! - **High Performance**: Direct access to DXGI Desktop Duplication API
//! - **Multiple Monitor Support**: Capture from any available display
//! - **Flexible Output**: Get pixel data as [`BGRA8`] or raw component bytes
//! - **Comprehensive Error Handling**: Robust error types for production use
//! - **Windows Optimized**: Specifically designed for Windows platforms
//!
//! # Platform Requirements
//!
//! - Windows 8 or later (DXGI 1.2+ required)
//! - Compatible graphics driver supporting Desktop Duplication
//! - Active desktop session (not suitable for headless environments)
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use dxgi_capture_rs::{DXGIManager, CaptureError};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a new DXGI manager with 1000ms timeout
//!     let mut manager = DXGIManager::new(1000)?;
//!     
//!     // Get screen dimensions
//!     let (width, height) = manager.geometry();
//!     println!("Screen: {}x{}", width, height);
//!     
//!     // Capture a frame
//!     match manager.capture_frame() {
//!         Ok((pixels, (frame_width, frame_height))) => {
//!             println!("Captured {}x{} frame with {} pixels",
//!                      frame_width, frame_height, pixels.len());
//!             
//!             // Process your pixel data here
//!             // pixels is Vec<BGRA8> where each pixel has b, g, r, a components
//!         }
//!         Err(CaptureError::Timeout) => {
//!             println!("Capture timed out - no new frame available");
//!         }
//!         Err(e) => {
//!             eprintln!("Capture failed: {:?}", e);
//!         }
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! # Multi-Monitor Support
//!
//! The library supports capturing from multiple monitors:
//!
//! ```rust,no_run
//! # use dxgi_capture_rs::DXGIManager;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut manager = DXGIManager::new(1000)?;
//!
//! // Capture from primary monitor (index 0)
//! manager.set_capture_source_index(0);
//! let (pixels, dimensions) = manager.capture_frame()?;
//!
//! // Capture from secondary monitor (index 1, if available)
//! manager.set_capture_source_index(1);
//! let (pixels, dimensions) = manager.capture_frame()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Error Handling
//!
//! The library provides comprehensive error handling for various scenarios:
//!
//! ```rust,no_run
//! # use dxgi_capture_rs::{DXGIManager, CaptureError};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut manager = DXGIManager::new(1000)?;
//!
//! match manager.capture_frame() {
//!     Ok((pixels, dimensions)) => {
//!         // Process successful capture
//!     }
//!     Err(CaptureError::Timeout) => {
//!         // No new frame available within timeout - normal occurrence
//!     }
//!     Err(CaptureError::AccessDenied) => {
//!         // Protected content (e.g., fullscreen video with DRM)
//!     }
//!     Err(CaptureError::AccessLost) => {
//!         // Display mode changed, need to reinitialize
//!     }
//!     Err(e) => {
//!         eprintln!("Capture failed: {:?}", e);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Performance Considerations
//!
//! - Use appropriate timeout values based on your frame rate requirements
//! - Consider using [`DXGIManager::capture_frame_components`] for raw byte data
//! - Memory usage scales with screen resolution
//! - The library automatically handles screen rotation
//!
//! # Thread Safety
//!
//! [`DXGIManager`] is not thread-safe. Create separate instances for each thread
//! if you need concurrent capture operations.

#![cfg(windows)]

extern crate winapi;
extern crate wio;

use std::mem::zeroed;
use std::{mem, ptr, slice};
use std::fmt;
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

/// A pixel color represented in BGRA8 format.
///
/// This structure represents a single pixel with Blue, Green, Red, and Alpha channels,
/// each stored as an 8-bit unsigned integer. This is the standard format used by
/// the DXGI Desktop Duplication API.
///
/// # Channel Order
///
/// The channels are ordered as BGRA (Blue, Green, Red, Alpha) to match the
/// Windows DXGI format. This differs from the more common RGBA ordering.
///
/// # Value Range
///
/// Each channel can hold values from 0 to 255:
/// - 0 represents the minimum intensity (black for color channels, transparent for alpha)
/// - 255 represents the maximum intensity (full color for color channels, opaque for alpha)
///
/// # Examples
///
/// ```rust
/// use dxgi_capture_rs::BGRA8;
///
/// // Create a red pixel (fully opaque)
/// let red_pixel = BGRA8 { b: 0, g: 0, r: 255, a: 255 };
///
/// // Create a semi-transparent blue pixel
/// let blue_pixel = BGRA8 { b: 255, g: 0, r: 0, a: 128 };
///
/// // Create a white pixel
/// let white_pixel = BGRA8 { b: 255, g: 255, r: 255, a: 255 };
///
/// // Create a transparent pixel (color doesn't matter when alpha is 0)
/// let transparent_pixel = BGRA8 { b: 0, g: 0, r: 0, a: 0 };
/// ```
#[derive(Copy, Clone, Debug, PartialOrd, PartialEq, Eq, Ord)]
pub struct BGRA8 {
    /// Blue channel (0-255)
    pub b: u8,
    /// Green channel (0-255)
    pub g: u8,
    /// Red channel (0-255)
    pub r: u8,
    /// Alpha channel (0-255, where 0 is transparent and 255 is opaque)
    pub a: u8,
}

/// Errors that can occur during screen capture operations.
///
/// This enum represents the various error conditions that can occur when
/// attempting to capture screen content using the DXGI Desktop Duplication API.
/// Each variant indicates a specific failure scenario and suggests appropriate
/// recovery strategies.
///
/// # Examples
///
/// ```rust,no_run
/// # use dxgi_capture_rs::{DXGIManager, CaptureError};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut manager = DXGIManager::new(1000)?;
///
/// match manager.capture_frame() {
///     Ok((pixels, dimensions)) => {
///         // Process successful capture
///         println!("Captured {}x{} frame", dimensions.0, dimensions.1);
///     }
///     Err(CaptureError::Timeout) => {
///         // No new frame available - this is normal
///         println!("No new frame available within timeout");
///     }
///     Err(CaptureError::AccessDenied) => {
///         // Protected content is being displayed
///         println!("Cannot capture protected content");
///     }
///     Err(CaptureError::AccessLost) => {
///         // Display mode changed, need to reinitialize
///         println!("Display mode changed, reinitializing...");
///         // You would typically recreate the manager here
///     }
///     Err(CaptureError::RefreshFailure) => {
///         // Failed to refresh after a previous error
///         println!("Failed to refresh capture system");
///     }
///     Err(CaptureError::Fail(msg)) => {
///         // General failure with specific message
///         println!("Capture failed: {}", msg);
///     }
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub enum CaptureError {
    /// Access to the output duplication was denied.
    ///
    /// This typically occurs when attempting to capture protected content,
    /// such as fullscreen video with DRM protection. The capture operation
    /// cannot proceed due to security restrictions.
    ///
    /// **Recovery Strategy**: Check if protected content is being displayed
    /// and inform the user that capture is not possible during protected playback.
    AccessDenied,

    /// Access to the duplicated output was lost.
    ///
    /// This occurs when the display configuration changes, such as:
    /// - Switching between windowed and fullscreen mode
    /// - Changing display resolution
    /// - Connecting/disconnecting monitors
    /// - Graphics driver updates
    ///
    /// **Recovery Strategy**: Recreate the [`DXGIManager`] instance to
    /// re-establish the connection to the updated display configuration.
    AccessLost,

    /// Failed to refresh the output duplication after a previous error.
    ///
    /// This indicates that the system attempted to recover from a previous
    /// error but was unsuccessful in re-establishing the capture session.
    ///
    /// **Recovery Strategy**: Recreate the [`DXGIManager`] instance or
    /// wait before attempting capture again.
    RefreshFailure,

    /// The capture operation timed out.
    ///
    /// This is a normal occurrence indicating that no new frame was available
    /// within the specified timeout period. This often happens when the screen
    /// content hasn't changed since the last capture.
    ///
    /// **Recovery Strategy**: This is not an error condition. Simply retry
    /// the capture operation. Consider adjusting the timeout value if needed.
    Timeout,

    /// A general or unexpected failure occurred.
    ///
    /// This represents various system-level failures that don't fit into
    /// the other specific error categories.
    ///
    /// **Recovery Strategy**: Log the error message and consider recreating
    /// the [`DXGIManager`] instance if the problem persists.
    Fail(&'static str),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::AccessDenied => write!(f, "Access to output duplication was denied"),
            CaptureError::AccessLost => write!(f, "Access to duplicated output was lost"),
            CaptureError::RefreshFailure => write!(f, "Failed to refresh output duplication"),
            CaptureError::Timeout => write!(f, "Capture operation timed out"),
            CaptureError::Fail(msg) => write!(f, "Capture failed: {}", msg),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Errors that can occur during output duplication initialization.
///
/// This enum represents errors that can occur when setting up the DXGI
/// Desktop Duplication system, typically during [`DXGIManager::new`] or
/// [`DXGIManager::acquire_output_duplication`] operations.
///
/// # Examples
///
/// ```rust,no_run
/// # use dxgi_capture_rs::{DXGIManager, OutputDuplicationError};
/// match DXGIManager::new(1000) {
///     Ok(manager) => {
///         // Successfully created manager
///         println!("DXGI manager created successfully");
///     }
///     Err(error) => {
///         // Handle initialization errors
///         match error {
///             "No suitable output found" => {
///                 println!("No displays available for capture");
///             }
///             "Failed to create device or duplicate output" => {
///                 println!("Graphics system initialization failed");
///             }
///             _ => {
///                 println!("Unexpected error: {}", error);
///             }
///         }
///     }
/// }
/// ```
#[derive(Debug)]
pub enum OutputDuplicationError {
    /// No suitable output display was found.
    ///
    /// This occurs when:
    /// - No displays are connected to the system
    /// - All displays are disconnected or disabled
    /// - The graphics driver doesn't support Desktop Duplication
    /// - Running in a headless environment (e.g., some CI systems)
    ///
    /// **Recovery Strategy**: Ensure that at least one display is connected
    /// and enabled, and that the graphics driver supports Desktop Duplication.
    NoOutput,

    /// Failed to create the D3D11 device or duplicate the output.
    ///
    /// This can occur due to:
    /// - Graphics driver issues
    /// - Insufficient system resources
    /// - Hardware acceleration disabled
    /// - Incompatible graphics hardware
    ///
    /// **Recovery Strategy**: Check graphics driver installation and system
    /// resources. Ensure hardware acceleration is enabled.
    DeviceError,
}

impl fmt::Display for OutputDuplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputDuplicationError::NoOutput => write!(f, "No suitable output display was found"),
            OutputDuplicationError::DeviceError => write!(f, "Failed to create D3D11 device or duplicate output"),
        }
    }
}

impl std::error::Error for OutputDuplicationError {}

/// Checks whether a Windows HRESULT represents a failure condition.
///
/// This utility function determines if a Windows HRESULT value indicates
/// a failure. In the Windows API, HRESULT values are 32-bit integers where
/// negative values indicate failures and non-negative values indicate success.
///
/// # Arguments
///
/// * `hr` - The HRESULT value to check
///
/// # Returns
///
/// `true` if the HRESULT represents a failure (negative value), `false` otherwise.
///
/// # Examples
///
/// ```rust
/// use dxgi_capture_rs::hr_failed;
/// use winapi::shared::winerror::{S_OK, E_FAIL};
///
/// // Success codes
/// assert!(!hr_failed(S_OK));        // 0
/// assert!(!hr_failed(1));           // Positive values are success
///
/// // Failure codes
/// assert!(hr_failed(E_FAIL));       // -2147467259
/// assert!(hr_failed(-1));           // Any negative value
/// ```
///
/// # Technical Details
///
/// The function simply checks if the HRESULT is negative (< 0). This works
/// because HRESULT uses the most significant bit as a severity flag:
/// - 0 = Success
/// - 1 = Failure
///
/// This is a standard Windows API pattern used throughout the DXGI and D3D11 APIs.
pub fn hr_failed(hr: HRESULT) -> bool {
    hr < 0
}

fn create_dxgi_factory_1() -> ComPtr<IDXGIFactory1> {
    unsafe {
        let mut factory = ptr::null_mut();
        let hr = CreateDXGIFactory1(&IID_IDXGIFactory1, &mut factory);
        if hr_failed(hr) {
            panic!("Failed to create DXGIFactory1, {hr:x}")
        } else {
            ComPtr::from_raw(factory as *mut IDXGIFactory1)
        }
    }
}

fn d3d11_create_device(
    adapter: *mut IDXGIAdapter,
) -> (ComPtr<ID3D11Device>, ComPtr<ID3D11DeviceContext>) {
    unsafe {
        let (mut d3d11_device, mut device_context) = (ptr::null_mut(), ptr::null_mut());
        let mut feature_level = D3D_FEATURE_LEVEL_9_1;
        let hr = D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            0,
            D3D11_SDK_VERSION,
            &mut d3d11_device,
            &mut feature_level,
            &mut device_context,
        );
        if hr_failed(hr) {
            panic!("Failed to create d3d11 device and device context, {hr:x}")
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
    output_dups: DuplicatedOutputs,
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

type DuplicatedOutputs = Vec<(ComPtr<IDXGIOutputDuplication>, ComPtr<IDXGIOutput1>)>;

fn duplicate_outputs(
    mut device: ComPtr<ID3D11Device>,
    outputs: Vec<ComPtr<IDXGIOutput>>,
) -> Result<(ComPtr<ID3D11Device>, DuplicatedOutputs), HRESULT> {
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

/// The main interface for DXGI Desktop Duplication screen capture.
///
/// `DXGIManager` provides a high-level interface to the Windows DXGI Desktop
/// Duplication API, enabling efficient screen capture operations. It manages
/// the underlying DXGI resources and provides methods to capture screen content
/// as pixel data.
///
/// # Usage
///
/// The typical workflow involves:
/// 1. Creating a manager with [`DXGIManager::new`]
/// 2. Optionally configuring the capture source and timeout
/// 3. Capturing frames using [`DXGIManager::capture_frame`] or [`DXGIManager::capture_frame_components`]
///
/// # Examples
///
/// ## Basic Usage
///
/// ```rust,no_run
/// use dxgi_capture_rs::DXGIManager;
///
/// let mut manager = DXGIManager::new(1000)?;
/// let (width, height) = manager.geometry();
///
/// match manager.capture_frame() {
///     Ok((pixels, (w, h))) => {
///         println!("Captured {}x{} frame with {} pixels", w, h, pixels.len());
///     }
///     Err(e) => {
///         eprintln!("Capture failed: {:?}", e);
///     }
/// }
/// # Ok::<(), &'static str>(())
/// ```
///
/// ## Multi-Monitor Setup
///
/// ```rust,no_run
/// use dxgi_capture_rs::DXGIManager;
///
/// let mut manager = DXGIManager::new(1000)?;
///
/// // Capture from primary monitor
/// manager.set_capture_source_index(0);
/// let primary_frame = manager.capture_frame();
///
/// // Capture from secondary monitor (if available)
/// manager.set_capture_source_index(1);
/// let secondary_frame = manager.capture_frame();
/// # Ok::<(), &'static str>(())
/// ```
///
/// ## Timeout Configuration
///
/// ```rust,no_run
/// use dxgi_capture_rs::DXGIManager;
///
/// let mut manager = DXGIManager::new(500)?;
///
/// // Adjust timeout for different scenarios
/// manager.set_timeout_ms(100);  // Fast polling
/// manager.set_timeout_ms(2000); // Slower polling
/// manager.set_timeout_ms(0);    // No timeout (immediate return)
/// # Ok::<(), &'static str>(())
/// ```
///
/// # Thread Safety
///
/// `DXGIManager` is not thread-safe. If you need to capture from multiple
/// threads, create separate instances for each thread.
///
/// # Resource Management
///
/// The manager automatically handles cleanup of DXGI resources when dropped.
/// However, if you encounter [`CaptureError::AccessLost`], you should create
/// a new manager instance to re-establish the connection to the display system.
pub struct DXGIManager {
    duplicated_output: Option<DuplicatedOutput>,
    capture_source_index: usize,
    timeout_ms: u32,
}

struct SharedPtr<T>(*const T);

unsafe impl<T> Send for SharedPtr<T> {}

unsafe impl<T> Sync for SharedPtr<T> {}

impl DXGIManager {
    /// Creates a new DXGI manager for screen capture operations.
    ///
    /// This constructor initializes the DXGI Desktop Duplication system and
    /// prepares it for screen capture operations. It automatically selects
    /// the primary display as the initial capture source.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - The timeout in milliseconds for capture operations.
    ///   - `0` means no timeout (immediate return if no frame available)
    ///   - Higher values wait longer for new frames
    ///   - Typical values: 1000-5000ms for interactive apps, 100-500ms for real-time
    ///
    /// # Returns
    ///
    /// Returns `Ok(DXGIManager)` on success, or `Err(&'static str)` if
    /// initialization fails.
    ///
    /// # Errors
    ///
    /// This function can fail if:
    /// - No suitable display outputs are available
    /// - DXGI Desktop Duplication is not supported
    /// - Graphics driver issues prevent initialization
    /// - Running in a headless environment
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// // Create manager with 1 second timeout
    /// let manager = DXGIManager::new(1000)?;
    ///
    /// // Create manager with no timeout (immediate return)
    /// let fast_manager = DXGIManager::new(0)?;
    ///
    /// // Create manager with longer timeout for slower systems
    /// let slow_manager = DXGIManager::new(5000)?;
    /// # Ok::<(), &'static str>(())
    /// ```
    ///
    /// # Platform Requirements
    ///
    /// - Windows 8 or later (DXGI 1.2+ required)
    /// - Active desktop session
    /// - Compatible graphics driver
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

    /// Gets the dimensions of the current capture source.
    ///
    /// Returns the width and height of the display being captured, in pixels.
    /// This corresponds to the resolution of the selected capture source.
    ///
    /// # Returns
    ///
    /// A tuple `(width, height)` where:
    /// - `width` is the horizontal resolution in pixels
    /// - `height` is the vertical resolution in pixels
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let manager = DXGIManager::new(1000)?;
    /// let (width, height) = manager.geometry();
    /// println!("Display resolution: {}x{}", width, height);
    ///
    /// // Calculate total pixels
    /// let total_pixels = width * height;
    /// println!("Total pixels: {}", total_pixels);
    /// # Ok::<(), &'static str>(())
    /// ```
    ///
    /// # Notes
    ///
    /// - The dimensions remain constant unless the display configuration changes
    /// - If the display configuration changes, you may need to create a new manager
    /// - This method is fast and can be called frequently without performance concerns
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

    /// Sets the capture source index to select which display to capture from.
    ///
    /// In multi-monitor setups, this method allows you to choose which display
    /// to capture from. Index 0 always refers to the primary display, while
    /// indices 1 and higher refer to secondary displays.
    ///
    /// # Arguments
    ///
    /// * `cs` - The capture source index:
    ///   - `0` = Primary display (default)
    ///   - `1` = First secondary display
    ///   - `2` = Second secondary display, etc.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// // Capture from primary display (default)
    /// manager.set_capture_source_index(0);
    /// let primary_frame = manager.capture_frame();
    ///
    /// // Switch to secondary display
    /// manager.set_capture_source_index(1);
    /// let secondary_frame = manager.capture_frame();
    /// # Ok::<(), &'static str>(())
    /// ```
    ///
    /// # Notes
    ///
    /// - Setting an invalid index (e.g., for a non-existent display) will not
    ///   cause an immediate error, but subsequent capture operations may fail
    /// - This method automatically reinitializes the capture system for the new display
    /// - The geometry may change when switching between displays of different resolutions
    pub fn set_capture_source_index(&mut self, cs: usize) {
        self.capture_source_index = cs;
        let _ = self.acquire_output_duplication();
    }

    /// Gets the current capture source index.
    ///
    /// Returns the index of the display currently being used for capture operations.
    ///
    /// # Returns
    ///
    /// The current capture source index:
    /// - `0` = Primary display
    /// - `1` = First secondary display  
    /// - `2` = Second secondary display, etc.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// // Initially set to primary display
    /// assert_eq!(manager.get_capture_source_index(), 0);
    ///
    /// // Switch to secondary display
    /// manager.set_capture_source_index(1);
    /// assert_eq!(manager.get_capture_source_index(), 1);
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn get_capture_source_index(&self) -> usize {
        self.capture_source_index
    }

    /// Sets the timeout for capture operations.
    ///
    /// This timeout determines how long capture operations will wait for a new
    /// frame to become available before returning with a timeout error.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - The timeout in milliseconds:
    ///   - `0` = No timeout (immediate return if no frame available)
    ///   - `1-1000` = Short timeout for real-time applications
    ///   - `1000-5000` = Medium timeout for interactive applications
    ///   - `>5000` = Long timeout for less frequent captures
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// // Set short timeout for real-time capture
    /// manager.set_timeout_ms(100);
    ///
    /// // Set no timeout for immediate return
    /// manager.set_timeout_ms(0);
    ///
    /// // Set longer timeout for less frequent captures
    /// manager.set_timeout_ms(5000);
    /// # Ok::<(), &'static str>(())
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Lower timeouts reduce latency but may increase CPU usage due to frequent polling
    /// - Higher timeouts reduce CPU usage but may increase latency
    /// - Timeout of 0 is useful for checking if a frame is immediately available
    pub fn set_timeout_ms(&mut self, timeout_ms: u32) {
        self.timeout_ms = timeout_ms
    }

    /// Gets the current timeout value for capture operations.
    ///
    /// Returns the timeout in milliseconds that capture operations will wait
    /// for a new frame to become available.
    ///
    /// # Returns
    ///
    /// The current timeout in milliseconds:
    /// - `0` = No timeout (immediate return)
    /// - `>0` = Timeout in milliseconds
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// // Check initial timeout
    /// assert_eq!(manager.get_timeout_ms(), 1000);
    ///
    /// // Change timeout and verify
    /// manager.set_timeout_ms(500);
    /// assert_eq!(manager.get_timeout_ms(), 500);
    /// # Ok::<(), &'static str>(())
    /// ```
    pub fn get_timeout_ms(&self) -> u32 {
        self.timeout_ms
    }

    /// Reinitializes the output duplication for the selected capture source.
    ///
    /// This method is automatically called when needed, but can be called manually
    /// to recover from certain error conditions. It reinitializes the DXGI
    /// Desktop Duplication system for the currently selected capture source.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or `Err(OutputDuplicationError)` if the
    /// reinitialization fails.
    ///
    /// # Errors
    ///
    /// - [`OutputDuplicationError::NoOutput`] if no suitable display is found
    /// - [`OutputDuplicationError::DeviceError`] if device creation fails
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::{DXGIManager, CaptureError};
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// // Manually reinitialize if needed
    /// match manager.acquire_output_duplication() {
    ///     Ok(()) => println!("Successfully reinitialized"),
    ///     Err(e) => println!("Failed to reinitialize: {:?}", e),
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    ///
    /// # Notes
    ///
    /// - This method is automatically called during manager creation
    /// - It's automatically called when switching capture sources
    /// - You typically don't need to call this manually unless recovering from errors
    pub fn acquire_output_duplication(&mut self) -> Result<(), OutputDuplicationError> {
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
            let (d3d11_device, output_duplications) = duplicate_outputs(d3d11_device, outputs)
                .map_err(|_| OutputDuplicationError::DeviceError)?;
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
        Err(OutputDuplicationError::NoOutput)
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
                // Handle stride padding by copying row by row
                let byte_output_width = byte_size(output_width);
                for row in mapped_pixels.chunks(byte_stride) {
                    pixel_buf.extend_from_slice(&row[..byte_output_width]);
                }
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
            n => unreachable!("Undefined DXGI_MODE_ROTATION: {n}"),
        }
        unsafe { frame_surface.Unmap() };
        Ok((pixel_buf, (output_width, output_height)))
    }

    /// Captures a frame from the current capture source as BGRA8 pixels.
    ///
    /// This method captures the current screen content and returns it as a vector
    /// of [`BGRA8`] pixels along with the frame dimensions. The method waits for
    /// a new frame to become available, up to the configured timeout.
    ///
    /// # Returns
    ///
    /// On success, returns `Ok((pixels, (width, height)))` where:
    /// - `pixels` is a `Vec<BGRA8>` containing the pixel data
    /// - `width` and `height` are the frame dimensions in pixels
    /// - The total number of pixels is `width * height`
    /// - Pixels are stored in row-major order (left-to-right, top-to-bottom)
    ///
    /// On failure, returns `Err(CaptureError)` - see [`CaptureError`] for details.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::{DXGIManager, CaptureError};
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// match manager.capture_frame() {
    ///     Ok((pixels, (width, height))) => {
    ///         println!("Captured {}x{} frame with {} pixels", width, height, pixels.len());
    ///         
    ///         // Process pixels
    ///         for (i, pixel) in pixels.iter().enumerate() {
    ///             // Each pixel has b, g, r, a components
    ///             println!("Pixel {}: R={}, G={}, B={}, A={}",
    ///                      i, pixel.r, pixel.g, pixel.b, pixel.a);
    ///         }
    ///         
    ///         // Calculate average color
    ///         let len = pixels.len() as u64;
    ///         let avg_r = pixels.iter().map(|p| p.r as u64).sum::<u64>() / len;
    ///         let avg_g = pixels.iter().map(|p| p.g as u64).sum::<u64>() / len;
    ///         let avg_b = pixels.iter().map(|p| p.b as u64).sum::<u64>() / len;
    ///         println!("Average color: R={}, G={}, B={}", avg_r, avg_g, avg_b);
    ///     }
    ///     Err(CaptureError::Timeout) => {
    ///         println!("No new frame available within timeout");
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Capture failed: {:?}", e);
    ///     }
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - The method automatically handles screen rotation
    /// - Memory usage is `width * height * 4` bytes
    /// - Consider using [`DXGIManager::capture_frame_components`] for raw byte access
    /// - The timeout setting affects how long this method waits for new frames
    ///
    /// # Error Conditions
    ///
    /// - [`CaptureError::Timeout`] - No new frame within timeout (normal)
    /// - [`CaptureError::AccessDenied`] - Protected content is being displayed
    /// - [`CaptureError::AccessLost`] - Display configuration changed
    /// - [`CaptureError::RefreshFailure`] - Failed to reinitialize after error
    /// - [`CaptureError::Fail`] - Other system-level failures
    pub fn capture_frame(&mut self) -> Result<(Vec<BGRA8>, (usize, usize)), CaptureError> {
        self.capture_frame_t()
    }

    /// Captures a frame from the current capture source as raw component bytes.
    ///
    /// This method captures the current screen content and returns it as a vector
    /// of raw bytes representing the pixel components. Each pixel is represented
    /// by 4 consecutive bytes in BGRA order.
    ///
    /// # Returns
    ///
    /// On success, returns `Ok((components, (width, height)))` where:
    /// - `components` is a `Vec<u8>` containing the raw pixel component data
    /// - `width` and `height` are the frame dimensions in pixels
    /// - The vector length is `width * height * 4` bytes
    /// - Components are stored as [B, G, R, A, B, G, R, A, ...] in row-major order
    ///
    /// On failure, returns `Err(CaptureError)` - see [`CaptureError`] for details.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::{DXGIManager, CaptureError};
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// match manager.capture_frame_components() {
    ///     Ok((components, (width, height))) => {
    ///         println!("Captured {}x{} frame with {} bytes", width, height, components.len());
    ///         
    ///         // Process raw components (4 bytes per pixel: B, G, R, A)
    ///         for pixel_idx in 0..(width * height) {
    ///             let base_idx = pixel_idx * 4;
    ///             let b = components[base_idx];
    ///             let g = components[base_idx + 1];
    ///             let r = components[base_idx + 2];
    ///             let a = components[base_idx + 3];
    ///             
    ///             // Process pixel components
    ///             if pixel_idx < 5 {  // Show first 5 pixels
    ///                 println!("Pixel {}: R={}, G={}, B={}, A={}", pixel_idx, r, g, b, a);
    ///             }
    ///         }
    ///         
    ///         // Convert to different formats
    ///         let mut rgb_data = Vec::with_capacity(width * height * 3);
    ///         for chunk in components.chunks(4) {
    ///             rgb_data.push(chunk[2]); // R
    ///             rgb_data.push(chunk[1]); // G
    ///             rgb_data.push(chunk[0]); // B
    ///         }
    ///         println!("Converted to RGB format: {} bytes", rgb_data.len());
    ///     }
    ///     Err(CaptureError::Timeout) => {
    ///         println!("No new frame available within timeout");
    ///     }
    ///     Err(e) => {
    ///         eprintln!("Capture failed: {:?}", e);
    ///     }
    /// }
    /// # Ok::<(), &'static str>(())
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - This method provides the most direct access to pixel data
    /// - Memory usage is `width * height * 4` bytes
    /// - Useful for interfacing with C libraries or custom pixel processing
    /// - No additional struct overhead compared to [`DXGIManager::capture_frame`]
    ///
    /// # Component Layout
    ///
    /// Each pixel is represented by 4 consecutive bytes:
    /// - Byte 0: Blue component (0-255)
    /// - Byte 1: Green component (0-255)
    /// - Byte 2: Red component (0-255)
    /// - Byte 3: Alpha component (0-255)
    ///
    /// # Error Conditions
    ///
    /// This method has the same error conditions as [`DXGIManager::capture_frame`]:
    /// - [`CaptureError::Timeout`] - No new frame within timeout (normal)
    /// - [`CaptureError::AccessDenied`] - Protected content is being displayed
    /// - [`CaptureError::AccessLost`] - Display configuration changed
    /// - [`CaptureError::RefreshFailure`] - Failed to reinitialize after error
    /// - [`CaptureError::Fail`] - Other system-level failures
    pub fn capture_frame_components(&mut self) -> Result<(Vec<u8>, (usize, usize)), CaptureError> {
        self.capture_frame_t()
    }
}
