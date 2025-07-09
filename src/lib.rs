//! High-performance screen capturing with DXGI Desktop Duplication API for Windows.
//!
//! This library provides a Rust interface to the Windows DXGI Desktop Duplication API,
//! enabling efficient screen capture with minimal performance overhead.
//!
//! # Features
//!
//! - **High Performance**: Direct access to DXGI Desktop Duplication API
//! - **Multiple Monitor Support**: Capture from any available display
//! - **Flexible Output**: Get pixel data as [`BGRA8`] or raw component bytes
//! - **Frame Metadata**: Access dirty rectangles, moved rectangles, and timing information
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
//!     let mut manager = DXGIManager::new(1000)?;
//!     
//!     match manager.capture_frame() {
//!         Ok((pixels, (width, height))) => {
//!             println!("Captured {}x{} frame", width, height);
//!             // Process pixels (Vec<BGRA8>)
//!         }
//!         Err(CaptureError::Timeout) => {
//!             // No new frame - normal occurrence
//!         }
//!         Err(e) => eprintln!("Capture failed: {:?}", e),
//!     }
//!     Ok(())
//! }
//! ```
//!
//! # Multi-Monitor Support
//!
//! ```rust,no_run
//! # use dxgi_capture_rs::DXGIManager;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut manager = DXGIManager::new(1000)?;
//!
//! manager.set_capture_source_index(0); // Primary monitor
//! let (pixels, dimensions) = manager.capture_frame()?;
//!
//! manager.set_capture_source_index(1); // Secondary monitor
//! let (pixels, dimensions) = manager.capture_frame()?;
//! # Ok(())
//! # }
//! ```
//!
//! # Frame Metadata for Streaming Applications
//!
//! The library provides detailed frame metadata including dirty rectangles and moved rectangles,
//! which is crucial for optimizing streaming and remote desktop applications.
//!
//! ```rust,no_run
//! # use dxgi_capture_rs::DXGIManager;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut manager = DXGIManager::new(1000)?;
//!
//! match manager.capture_frame_with_metadata() {
//!     Ok((pixels, (width, height), metadata)) => {
//!         // Only process frame if there are actual changes
//!         if metadata.has_updates() {
//!             println!("Frame has {} dirty rects and {} move rects",
//!                      metadata.dirty_rects.len(), metadata.move_rects.len());
//!             
//!             // Process moved rectangles first (as per Microsoft recommendation)
//!             for move_rect in &metadata.move_rects {
//!                 let (src_x, src_y) = move_rect.source_point;
//!                 let (dst_left, dst_top, dst_right, dst_bottom) = move_rect.destination_rect;
//!                 
//!                 // Copy pixels from source to destination
//!                 // This is much more efficient than re-encoding the entire area
//!                 copy_rectangle(&pixels, src_x, src_y, dst_left, dst_top,
//!                               dst_right - dst_left, dst_bottom - dst_top);
//!             }
//!             
//!             // Then process dirty rectangles
//!             for &(left, top, right, bottom) in &metadata.dirty_rects {
//!                 let width = (right - left) as usize;
//!                 let height = (bottom - top) as usize;
//!                 
//!                 // Only encode/transmit the changed region
//!                 encode_region(&pixels, left as usize, top as usize, width, height);
//!             }
//!         }
//!         
//!         // Check for mouse cursor updates
//!         if metadata.has_mouse_updates() {
//!             if let Some((x, y)) = metadata.pointer_position {
//!                 println!("Mouse cursor at ({}, {}), visible: {}", x, y, metadata.pointer_visible);
//!             }
//!         }
//!     }
//!     Err(e) => eprintln!("Capture failed: {:?}", e),
//! }
//!
//! # fn copy_rectangle(pixels: &[dxgi_capture_rs::BGRA8], src_x: i32, src_y: i32,
//! #                   dst_x: i32, dst_y: i32, width: i32, height: i32) {}
//! # fn encode_region(pixels: &[dxgi_capture_rs::BGRA8], x: usize, y: usize, width: usize, height: usize) {}
//! # Ok(())
//! # }
//! ```
//!
//! # Error Handling
//!
//! ```rust,no_run
//! # use dxgi_capture_rs::{DXGIManager, CaptureError};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut manager = DXGIManager::new(1000)?;
//!
//! match manager.capture_frame() {
//!     Ok((pixels, dimensions)) => { /* Process successful capture */ }
//!     Err(CaptureError::Timeout) => { /* No new frame - normal */ }
//!     Err(CaptureError::AccessDenied) => { /* Protected content */ }
//!     Err(CaptureError::AccessLost) => { /* Display mode changed */ }
//!     Err(e) => eprintln!("Capture failed: {:?}", e),
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
//! - Use metadata to optimize streaming by only processing changed regions
//! - Process move rectangles before dirty rectangles for correct visual output
//!
//! # Thread Safety
//!
//! [`DXGIManager`] is not thread-safe. Create separate instances for each thread
//! if you need concurrent capture operations.

#![cfg(windows)]
#![cfg_attr(docsrs, doc(cfg(windows)))]

use std::fmt;
use std::{mem, slice};
use windows::{
    Win32::{
        Foundation::{HMODULE, RECT},
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_9_1},
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
                ID3D11DeviceContext, ID3D11Texture2D,
            },
            Dxgi::{
                Common::{
                    DXGI_MODE_ROTATION_IDENTITY, DXGI_MODE_ROTATION_ROTATE90,
                    DXGI_MODE_ROTATION_ROTATE180, DXGI_MODE_ROTATION_ROTATE270,
                    DXGI_MODE_ROTATION_UNSPECIFIED,
                },
                CreateDXGIFactory1, DXGI_ERROR_ACCESS_DENIED, DXGI_ERROR_ACCESS_LOST,
                DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, DXGI_ERROR_WAIT_TIMEOUT, DXGI_MAP_READ,
                DXGI_MAPPED_RECT, DXGI_OUTDUPL_FRAME_INFO, DXGI_OUTDUPL_MOVE_RECT,
                DXGI_OUTPUT_DESC, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput,
                IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource, IDXGISurface1,
            },
        },
    },
    core::{Interface, Result as WindowsResult},
};

/// A pixel color in BGRA8 format.
///
/// Each channel can hold values from 0 to 255. The channels are ordered as BGRA
/// to match the Windows DXGI format.
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

/// Represents a rectangle that has been moved from one location to another.
///
/// This structure describes a region that was moved from a source location to
/// a destination location, which is useful for optimizing screen updates in
/// streaming applications.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MoveRect {
    /// The source point where the content was moved from (top-left corner)
    pub source_point: (i32, i32),
    /// The destination rectangle where the content was moved to
    pub destination_rect: (i32, i32, i32, i32), // (left, top, right, bottom)
}

/// Metadata about a captured frame.
///
/// This structure contains timing information, dirty regions, moved regions,
/// and other metadata that can help optimize screen capture and streaming
/// applications.
#[derive(Clone, Debug)]
pub struct FrameMetadata {
    /// Timestamp of the last desktop image update (Windows performance counter)
    pub last_present_time: i64,
    /// Timestamp of the last mouse update (Windows performance counter)
    pub last_mouse_update_time: i64,
    /// Number of frames accumulated since the last processed frame
    pub accumulated_frames: u32,
    /// Whether dirty regions were coalesced and may contain unmodified pixels
    pub rects_coalesced: bool,
    /// Whether protected content was masked out in the captured frame
    pub protected_content_masked_out: bool,
    /// Mouse cursor position and visibility
    pub pointer_position: Option<(i32, i32)>,
    /// Whether the mouse cursor is visible
    pub pointer_visible: bool,
    /// List of dirty rectangles that have changed since the last frame
    pub dirty_rects: Vec<(i32, i32, i32, i32)>, // (left, top, right, bottom)
    /// List of move rectangles that have been moved since the last frame
    pub move_rects: Vec<MoveRect>,
}

impl FrameMetadata {
    /// Returns true if the frame contains any updates (dirty regions or moves)
    pub fn has_updates(&self) -> bool {
        !self.dirty_rects.is_empty() || !self.move_rects.is_empty()
    }

    /// Returns true if the mouse cursor has been updated
    pub fn has_mouse_updates(&self) -> bool {
        self.last_mouse_update_time > 0
    }

    /// Returns the total number of changed regions
    pub fn total_change_count(&self) -> usize {
        self.dirty_rects.len() + self.move_rects.len()
    }
}

/// Errors that can occur during screen capture operations.
#[derive(Debug)]
pub enum CaptureError {
    /// Access to the output duplication was denied.
    ///
    /// This typically occurs when attempting to capture protected content,
    /// such as fullscreen video with DRM protection.
    ///
    /// **Recovery**: Check if protected content is being displayed.
    AccessDenied,

    /// Access to the duplicated output was lost.
    ///
    /// This occurs when the display configuration changes, such as:
    /// - Switching between windowed and fullscreen mode
    /// - Changing display resolution
    /// - Connecting/disconnecting monitors
    /// - Graphics driver updates
    ///
    /// **Recovery**: Recreate the [`DXGIManager`] instance.
    AccessLost,

    /// Failed to refresh the output duplication after a previous error.
    ///
    /// **Recovery**: Recreate the [`DXGIManager`] instance or wait before retrying.
    RefreshFailure,

    /// The capture operation timed out.
    ///
    /// This is a normal occurrence indicating that no new frame was available
    /// within the specified timeout period.
    ///
    /// **Recovery**: This is not an error condition. Simply retry the capture.
    Timeout,

    /// A general or unexpected failure occurred.
    ///
    /// **Recovery**: Log the error message and consider recreating the [`DXGIManager`].
    Fail(windows::core::Error),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::AccessDenied => write!(f, "Access to output duplication was denied"),
            CaptureError::AccessLost => write!(f, "Access to duplicated output was lost"),
            CaptureError::RefreshFailure => write!(f, "Failed to refresh output duplication"),
            CaptureError::Timeout => write!(f, "Capture operation timed out"),
            CaptureError::Fail(msg) => write!(f, "Capture failed: {msg}"),
        }
    }
}

impl std::error::Error for CaptureError {}

impl From<windows::core::Error> for CaptureError {
    fn from(err: windows::core::Error) -> Self {
        CaptureError::Fail(err)
    }
}

/// Errors that can occur during output duplication initialization.
#[derive(Debug)]
pub enum OutputDuplicationError {
    /// No suitable output display was found.
    ///
    /// This occurs when no displays are connected, all displays are disabled,
    /// or the graphics driver doesn't support Desktop Duplication.
    ///
    /// **Recovery**: Ensure a display is connected and graphics drivers support Desktop Duplication.
    NoOutput,

    /// Failed to create the D3D11 device or duplicate the output.
    ///
    /// This can occur due to graphics driver issues, insufficient system resources,
    /// or incompatible graphics hardware.
    ///
    /// **Recovery**: Check graphics driver installation and system resources.
    DeviceError(windows::core::Error),
}

impl fmt::Display for OutputDuplicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputDuplicationError::NoOutput => write!(f, "No suitable output display was found"),
            OutputDuplicationError::DeviceError(err) => {
                write!(f, "Failed to create D3D11 device: {err}")
            }
        }
    }
}

impl std::error::Error for OutputDuplicationError {}

impl From<windows::core::Error> for OutputDuplicationError {
    fn from(err: windows::core::Error) -> Self {
        OutputDuplicationError::DeviceError(err)
    }
}

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
/// use windows::core::HRESULT;
/// use windows::Win32::Foundation::{S_OK, E_FAIL};
///
/// // Success codes
/// assert!(!hr_failed(S_OK));        // 0
/// assert!(!hr_failed(HRESULT(1)));  // Positive values are success
///
/// // Failure codes
/// assert!(hr_failed(E_FAIL));       // -2147467259
/// assert!(hr_failed(HRESULT(-1)));  // Any negative value
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
///
/// # Safety
///
/// This function is unsafe because it involves a raw pointer dereference.
pub fn hr_failed(hr: windows::core::HRESULT) -> bool {
    hr.is_err()
}

fn create_dxgi_factory_1() -> WindowsResult<IDXGIFactory1> {
    unsafe { CreateDXGIFactory1() }
}

fn d3d11_create_device(
    adapter: Option<&IDXGIAdapter>,
) -> WindowsResult<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device: Option<ID3D11Device> = None;
    let mut device_context: Option<ID3D11DeviceContext> = None;
    let feature_levels = [D3D_FEATURE_LEVEL_9_1];

    unsafe {
        D3D11CreateDevice(
            adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut device_context),
        )
    }?;

    Ok((device.unwrap(), device_context.unwrap()))
}

fn get_adapter_outputs(adapter: &IDXGIAdapter1) -> WindowsResult<Vec<IDXGIOutput>> {
    let mut outputs = Vec::new();
    for i in 0.. {
        match unsafe { adapter.EnumOutputs(i) } {
            Ok(output) => {
                let desc: DXGI_OUTPUT_DESC = unsafe { output.GetDesc()? };
                if desc.AttachedToDesktop.as_bool() {
                    outputs.push(output);
                }
            }
            Err(_) => break,
        }
    }
    Ok(outputs)
}

fn get_capture_source(
    output_dups: &DuplicatedOutputs,
    cs_index: usize,
) -> Option<(IDXGIOutputDuplication, IDXGIOutput1)> {
    if cs_index < output_dups.len() {
        Some(output_dups[cs_index].clone())
    } else {
        None
    }
}

type DuplicatedOutputs = Vec<(IDXGIOutputDuplication, IDXGIOutput1)>;

fn duplicate_outputs(
    device: &ID3D11Device,
    outputs: Vec<IDXGIOutput>,
) -> WindowsResult<DuplicatedOutputs> {
    let mut duplicated_outputs = Vec::new();

    for output in outputs {
        let output1: IDXGIOutput1 = output.cast()?;
        let duplicated_output = unsafe { output1.DuplicateOutput(device)? };
        duplicated_outputs.push((duplicated_output, output1));
    }

    Ok(duplicated_outputs)
}

struct DuplicatedOutput {
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    output: IDXGIOutput1,
    output_duplication: IDXGIOutputDuplication,
}

impl DuplicatedOutput {
    fn get_desc(&self) -> WindowsResult<DXGI_OUTPUT_DESC> {
        unsafe { self.output.GetDesc() }
    }

    fn capture_frame_to_surface(&mut self, timeout_ms: u32) -> WindowsResult<IDXGISurface1> {
        let mut resource: Option<IDXGIResource> = None;
        let mut frame_info = unsafe { mem::zeroed() };

        unsafe {
            self.output_duplication
                .AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource)?
        };

        let texture: ID3D11Texture2D = resource.unwrap().cast()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;

        let mut staged_texture: Option<ID3D11Texture2D> = None;
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staged_texture))?
        };
        let staged_texture = staged_texture.unwrap();

        unsafe { self.device_context.CopyResource(&staged_texture, &texture) };

        unsafe { self.output_duplication.ReleaseFrame()? };

        let surface: IDXGISurface1 = staged_texture.cast()?;
        Ok(surface)
    }

    fn capture_frame_to_surface_with_metadata(
        &mut self,
        timeout_ms: u32,
    ) -> WindowsResult<(IDXGISurface1, FrameMetadata)> {
        let mut resource: Option<IDXGIResource> = None;
        let mut frame_info: DXGI_OUTDUPL_FRAME_INFO = unsafe { mem::zeroed() };

        unsafe {
            self.output_duplication
                .AcquireNextFrame(timeout_ms, &mut frame_info, &mut resource)?
        };

        // Extract metadata from frame_info
        let metadata = self.extract_frame_metadata(&frame_info)?;

        let texture: ID3D11Texture2D = resource.unwrap().cast()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { texture.GetDesc(&mut desc) };
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        desc.MiscFlags = 0;

        let mut staged_texture: Option<ID3D11Texture2D> = None;
        unsafe {
            self.device
                .CreateTexture2D(&desc, None, Some(&mut staged_texture))?
        };
        let staged_texture = staged_texture.unwrap();

        unsafe { self.device_context.CopyResource(&staged_texture, &texture) };

        unsafe { self.output_duplication.ReleaseFrame()? };

        let surface: IDXGISurface1 = staged_texture.cast()?;
        Ok((surface, metadata))
    }

    fn extract_frame_metadata(
        &self,
        frame_info: &DXGI_OUTDUPL_FRAME_INFO,
    ) -> WindowsResult<FrameMetadata> {
        let mut dirty_rects = Vec::new();
        let mut move_rects = Vec::new();

        // Get dirty rectangles if there are any
        if frame_info.TotalMetadataBufferSize > 0 {
            // Get dirty rectangles
            let mut dirty_rects_buffer_size = 0u32;
            let dirty_result = unsafe {
                self.output_duplication.GetFrameDirtyRects(
                    0,
                    std::ptr::null_mut(),
                    &mut dirty_rects_buffer_size,
                )
            };

            // Handle the case where there are dirty rects
            if dirty_result.is_ok() && dirty_rects_buffer_size > 0 {
                let dirty_rect_count = dirty_rects_buffer_size / mem::size_of::<RECT>() as u32;
                let mut dirty_rects_buffer: Vec<RECT> =
                    vec![RECT::default(); dirty_rect_count as usize];
                unsafe {
                    let get_result = self.output_duplication.GetFrameDirtyRects(
                        dirty_rects_buffer_size,
                        dirty_rects_buffer.as_mut_ptr(),
                        &mut dirty_rects_buffer_size,
                    );
                    if get_result.is_ok() {
                        dirty_rects = dirty_rects_buffer
                            .into_iter()
                            .map(|rect| (rect.left, rect.top, rect.right, rect.bottom))
                            .collect();
                    }
                }
            }

            // Get move rectangles
            let mut move_rects_buffer_size = 0u32;
            let move_result = unsafe {
                self.output_duplication.GetFrameMoveRects(
                    0,
                    std::ptr::null_mut(),
                    &mut move_rects_buffer_size,
                )
            };

            // Handle the case where there are move rects
            if move_result.is_ok() && move_rects_buffer_size > 0 {
                let move_rect_count =
                    move_rects_buffer_size / mem::size_of::<DXGI_OUTDUPL_MOVE_RECT>() as u32;
                let mut move_rects_buffer: Vec<DXGI_OUTDUPL_MOVE_RECT> =
                    vec![unsafe { mem::zeroed() }; move_rect_count as usize];
                unsafe {
                    let get_result = self.output_duplication.GetFrameMoveRects(
                        move_rects_buffer_size,
                        move_rects_buffer.as_mut_ptr(),
                        &mut move_rects_buffer_size,
                    );
                    if get_result.is_ok() {
                        move_rects = move_rects_buffer
                            .into_iter()
                            .map(|move_rect| MoveRect {
                                source_point: (move_rect.SourcePoint.x, move_rect.SourcePoint.y),
                                destination_rect: (
                                    move_rect.DestinationRect.left,
                                    move_rect.DestinationRect.top,
                                    move_rect.DestinationRect.right,
                                    move_rect.DestinationRect.bottom,
                                ),
                            })
                            .collect();
                    }
                }
            }
        }

        let pointer_position = if frame_info.PointerPosition.Visible.as_bool() {
            Some((
                frame_info.PointerPosition.Position.x,
                frame_info.PointerPosition.Position.y,
            ))
        } else {
            None
        };

        Ok(FrameMetadata {
            last_present_time: frame_info.LastPresentTime,
            last_mouse_update_time: frame_info.LastMouseUpdateTime,
            accumulated_frames: frame_info.AccumulatedFrames,
            rects_coalesced: frame_info.RectsCoalesced.as_bool(),
            protected_content_masked_out: frame_info.ProtectedContentMaskedOut.as_bool(),
            pointer_position,
            pointer_visible: frame_info.PointerPosition.Visible.as_bool(),
            dirty_rects,
            move_rects,
        })
    }
}

/// The main manager for handling DXGI desktop duplication.
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
/// # Ok::<(), Box<dyn std::error::Error>>(())
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
/// # Ok::<(), Box<dyn std::error::Error>>(())
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
/// # Ok::<(), Box<dyn std::error::Error>>(())
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
    factory: IDXGIFactory1,
    duplicated_output: Option<DuplicatedOutput>,
    capture_source_index: usize,
    timeout_ms: u32,
}

impl DXGIManager {
    /// Creates a new `DXGIManager` instance.
    ///
    /// This initializes the DXGI factory and sets up the necessary resources
    /// for screen capture. The `timeout_ms` parameter specifies the default
    /// timeout for frame capture operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the DXGI manager cannot be initialized, which
    /// typically occurs if the required graphics components are not available.
    pub fn new(timeout_ms: u32) -> Result<Self, OutputDuplicationError> {
        let factory = create_dxgi_factory_1()?;
        let mut manager = Self {
            factory,
            duplicated_output: None,
            capture_source_index: 0,
            timeout_ms,
        };
        manager.acquire_output_duplication()?;
        Ok(manager)
    }

    /// Returns the screen geometry (width, height) of the current capture source.
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Notes
    ///
    /// - The dimensions remain constant unless the display configuration changes
    /// - If the display configuration changes, you may need to create a new manager
    /// - This method is fast and can be called frequently without performance concerns
    pub fn geometry(&self) -> (usize, usize) {
        if let Some(ref output) = self.duplicated_output {
            let output_desc = output.get_desc().expect("Failed to get output description");
            let RECT {
                left,
                top,
                right,
                bottom,
            } = output_desc.DesktopCoordinates;
            ((right - left) as usize, (bottom - top) as usize)
        } else {
            // This should not happen if the manager was properly initialized
            // Return (0, 0) to prevent panic, but this indicates a serious issue
            (0, 0)
        }
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Notes
    ///
    /// - Setting an invalid index (e.g., for a non-existent display) will not
    ///   cause an immediate error, but subsequent capture operations may fail
    /// - This method automatically reinitializes the capture system for the new display
    /// - The geometry may change when switching between displays of different resolutions
    pub fn set_capture_source_index(&mut self, cs: usize) {
        let previous_index = self.capture_source_index;
        self.capture_source_index = cs;

        // Try to acquire the new capture source
        if self.acquire_output_duplication().is_err() {
            // If it fails and we're switching to index 0 (primary), revert to previous index
            // This helps recover from invalid secondary display switches
            if cs == 0 && cs != previous_index {
                self.capture_source_index = previous_index;
                let _ = self.acquire_output_duplication();
            }
        }
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
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
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Notes
    ///
    /// - This method is automatically called during manager creation
    /// - It's automatically called when switching capture sources
    /// - You typically don't need to call this manually unless recovering from errors
    pub fn acquire_output_duplication(&mut self) -> Result<(), OutputDuplicationError> {
        self.duplicated_output = None;

        for i in 0.. {
            let adapter = match unsafe { self.factory.EnumAdapters1(i) } {
                Ok(adapter) => adapter,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(e.into()),
            };

            let (d3d11_device, device_context) = match d3d11_create_device(Some(&adapter.cast()?)) {
                Ok(device) => device,
                Err(_) => continue,
            };

            let outputs = get_adapter_outputs(&adapter)?;
            if outputs.is_empty() {
                continue;
            }

            let output_duplications = duplicate_outputs(&d3d11_device, outputs)?;

            if let Some((output_duplication, output)) =
                get_capture_source(&output_duplications, self.capture_source_index)
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

    fn capture_frame_to_surface(&mut self) -> Result<IDXGISurface1, CaptureError> {
        if self.duplicated_output.is_none() && self.acquire_output_duplication().is_err() {
            return Err(CaptureError::RefreshFailure);
        }

        let duplicated_output = self.duplicated_output.as_mut().unwrap();

        match duplicated_output.capture_frame_to_surface(self.timeout_ms) {
            Ok(surface) => Ok(surface),
            Err(e) => {
                let code = e.code();
                if code == DXGI_ERROR_ACCESS_LOST {
                    self.duplicated_output = None;
                    Err(CaptureError::AccessLost)
                } else if code == DXGI_ERROR_WAIT_TIMEOUT {
                    Err(CaptureError::Timeout)
                } else if code == DXGI_ERROR_ACCESS_DENIED {
                    self.duplicated_output = None;
                    Err(CaptureError::AccessDenied)
                } else {
                    self.duplicated_output = None;
                    Err(CaptureError::Fail(e))
                }
            }
        }
    }

    fn capture_frame_to_surface_with_metadata(
        &mut self,
    ) -> Result<(IDXGISurface1, FrameMetadata), CaptureError> {
        if self.duplicated_output.is_none() && self.acquire_output_duplication().is_err() {
            return Err(CaptureError::RefreshFailure);
        }

        let duplicated_output = self.duplicated_output.as_mut().unwrap();

        match duplicated_output.capture_frame_to_surface_with_metadata(self.timeout_ms) {
            Ok((surface, metadata)) => Ok((surface, metadata)),
            Err(e) => {
                let code = e.code();
                if code == DXGI_ERROR_ACCESS_LOST {
                    self.duplicated_output = None;
                    Err(CaptureError::AccessLost)
                } else if code == DXGI_ERROR_WAIT_TIMEOUT {
                    Err(CaptureError::Timeout)
                } else if code == DXGI_ERROR_ACCESS_DENIED {
                    self.duplicated_output = None;
                    Err(CaptureError::AccessDenied)
                } else if code == DXGI_ERROR_MORE_DATA {
                    // This should not happen with proper buffer sizing, but handle it gracefully
                    self.duplicated_output = None;
                    Err(CaptureError::Fail(e))
                } else {
                    self.duplicated_output = None;
                    Err(CaptureError::Fail(e))
                }
            }
        }
    }

    fn capture_frame_t<T: Copy + Send + Sync + Sized>(
        &mut self,
    ) -> Result<(Vec<T>, (usize, usize)), CaptureError> {
        let surface = self.capture_frame_to_surface()?;

        let mut rect = DXGI_MAPPED_RECT::default();
        unsafe { surface.Map(&mut rect, DXGI_MAP_READ)? };

        let desc = self
            .duplicated_output
            .as_ref()
            .ok_or(CaptureError::RefreshFailure)?
            .get_desc()?;
        let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as usize;
        let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as usize;

        let pitch = rect.Pitch as usize;
        let source = rect.pBits;

        let (rotated_width, rotated_height) = match desc.Rotation {
            DXGI_MODE_ROTATION_ROTATE90 | DXGI_MODE_ROTATION_ROTATE270 => (height, width),
            _ => (width, height),
        };

        let mut data_vec: Vec<T> = Vec::with_capacity(
            rotated_width * rotated_height * mem::size_of::<BGRA8>() / mem::size_of::<T>(),
        );

        let bytes_per_pixel = mem::size_of::<BGRA8>() / mem::size_of::<T>();
        let source_slice = unsafe {
            slice::from_raw_parts(source as *const T, pitch * height / mem::size_of::<T>())
        };

        match desc.Rotation {
            DXGI_MODE_ROTATION_IDENTITY | DXGI_MODE_ROTATION_UNSPECIFIED => {
                for i in 0..height {
                    let start = i * pitch / mem::size_of::<T>();
                    let end = start + width * bytes_per_pixel;
                    data_vec.extend_from_slice(&source_slice[start..end]);
                }
            }
            DXGI_MODE_ROTATION_ROTATE90 => {
                for i in 0..width {
                    for j in (0..height).rev() {
                        let index = j * pitch / mem::size_of::<T>() + i * bytes_per_pixel;
                        data_vec.extend_from_slice(&source_slice[index..index + bytes_per_pixel]);
                    }
                }
            }
            DXGI_MODE_ROTATION_ROTATE180 => {
                for i in (0..height).rev() {
                    for j in (0..width).rev() {
                        let index = i * pitch / mem::size_of::<T>() + j * bytes_per_pixel;
                        data_vec.extend_from_slice(&source_slice[index..index + bytes_per_pixel]);
                    }
                }
            }
            DXGI_MODE_ROTATION_ROTATE270 => {
                for i in (0..width).rev() {
                    for j in 0..height {
                        let index = j * pitch / mem::size_of::<T>() + i * bytes_per_pixel;
                        data_vec.extend_from_slice(&source_slice[index..index + bytes_per_pixel]);
                    }
                }
            }
            _ => {}
        }

        unsafe { surface.Unmap()? };

        Ok((data_vec, (rotated_width, rotated_height)))
    }

    /// Captures a single frame and returns it as a `Vec<BGRA8>`.
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
    /// - Pixels are stored in row-major order (left-to-right, top-to-bottom)
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
    ///         // Process pixels - each pixel has b, g, r, a components
    ///     }
    ///     Err(CaptureError::Timeout) => {
    ///         // No new frame available within timeout
    ///     }
    ///     Err(e) => eprintln!("Capture failed: {:?}", e),
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Automatically handles screen rotation
    /// - Memory usage is `width * height * 4` bytes
    /// - Consider using [`DXGIManager::capture_frame_components`] for raw byte access
    pub fn capture_frame(&mut self) -> Result<(Vec<BGRA8>, (usize, usize)), CaptureError> {
        self.capture_frame_t()
    }

    /// Captures a single frame and returns it as a `Vec<u8>`.
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
    /// - Components are stored as [B, G, R, A, B, G, R, A, ...] in row-major order
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// match manager.capture_frame_components() {
    ///     Ok((components, (width, height))) => {
    ///         println!("Captured {}x{} frame with {} bytes", width, height, components.len());
    ///         // Process raw components (4 bytes per pixel: B, G, R, A)
    ///     }
    ///     Err(e) => eprintln!("Capture failed: {:?}", e),
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Provides direct access to pixel data
    /// - Memory usage is `width * height * 4` bytes
    /// - Useful for interfacing with C libraries or custom pixel processing
    pub fn capture_frame_components(&mut self) -> Result<(Vec<u8>, (usize, usize)), CaptureError> {
        self.capture_frame_t()
    }

    /// Captures a single frame with minimal overhead for performance-critical applications.
    ///
    /// This method provides the fastest possible screen capture by minimizing memory
    /// allocations and copying. Returns raw pixel data without rotation handling.
    ///
    /// # Returns
    ///
    /// On success, returns `Ok((pixels, (width, height)))` where:
    /// - `pixels` is a `Vec<u8>` containing raw BGRA pixel data
    /// - `width` and `height` are the frame dimensions in pixels
    /// - Data is in the native orientation (no rotation correction)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let mut manager = DXGIManager::new(100)?;
    ///
    /// match manager.capture_frame_fast() {
    ///     Ok((pixels, (width, height))) => {
    ///         println!("Fast captured {}x{} frame", width, height);
    ///         // Process raw BGRA data directly
    ///     }
    ///     Err(e) => eprintln!("Fast capture failed: {:?}", e),
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn capture_frame_fast(&mut self) -> Result<(Vec<u8>, (usize, usize)), CaptureError> {
        let surface = self.capture_frame_to_surface()?;

        let mut rect = DXGI_MAPPED_RECT::default();
        unsafe { surface.Map(&mut rect, DXGI_MAP_READ)? };

        let desc = self
            .duplicated_output
            .as_ref()
            .ok_or(CaptureError::RefreshFailure)?
            .get_desc()?;
        let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as usize;
        let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as usize;

        let pitch = rect.Pitch as usize;
        let source = rect.pBits;

        // Fast path: copy data without rotation handling for maximum performance
        let bytes_per_row = width * 4; // 4 bytes per pixel (BGRA)
        let mut data_vec = Vec::with_capacity(width * height * 4);

        unsafe {
            // Ultra-fast path: if pitch equals row width, copy everything at once
            if pitch == bytes_per_row {
                let total_bytes = width * height * 4;
                let source_slice = slice::from_raw_parts(source as *const u8, total_bytes);
                data_vec.extend_from_slice(source_slice);
            } else {
                // Standard path: copy row by row for different pitch
                let source_slice = slice::from_raw_parts(source as *const u8, pitch * height);

                for row in 0..height {
                    let row_start = row * pitch;
                    let row_end = row_start + bytes_per_row;
                    data_vec.extend_from_slice(&source_slice[row_start..row_end]);
                }
            }
        }

        unsafe { surface.Unmap()? };

        Ok((data_vec, (width, height)))
    }

    /// Captures a single frame and returns it as `Vec<BGRA8>` along with frame metadata.
    ///
    /// This method captures the current screen content and returns it as a vector
    /// of [`BGRA8`] pixels along with comprehensive metadata about the frame, including
    /// dirty rectangles, moved rectangles, and timing information.
    ///
    /// # Returns
    ///
    /// On success, returns `Ok((pixels, (width, height), metadata))` where:
    /// - `pixels` is a `Vec<BGRA8>` containing the pixel data
    /// - `width` and `height` are the frame dimensions in pixels
    /// - `metadata` is a [`FrameMetadata`] struct containing detailed frame information
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::{DXGIManager, CaptureError};
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// match manager.capture_frame_with_metadata() {
    ///     Ok((pixels, (width, height), metadata)) => {
    ///         println!("Captured {}x{} frame with {} dirty rects and {} move rects",
    ///                  width, height, metadata.dirty_rects.len(), metadata.move_rects.len());
    ///         
    ///         // Process only dirty regions for efficient streaming
    ///         for (left, top, right, bottom) in &metadata.dirty_rects {
    ///             println!("Dirty region: ({}, {}) to ({}, {})", left, top, right, bottom);
    ///         }
    ///         
    ///         // Handle moved content
    ///         for move_rect in &metadata.move_rects {
    ///             println!("Content moved from ({}, {}) to ({}, {}, {}, {})",
    ///                      move_rect.source_point.0, move_rect.source_point.1,
    ///                      move_rect.destination_rect.0, move_rect.destination_rect.1,
    ///                      move_rect.destination_rect.2, move_rect.destination_rect.3);
    ///         }
    ///     }
    ///     Err(CaptureError::Timeout) => {
    ///         // No new frame available within timeout
    ///     }
    ///     Err(e) => eprintln!("Capture failed: {:?}", e),
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// # Performance Notes
    ///
    /// - Automatically handles screen rotation
    /// - Memory usage is `width * height * 4` bytes plus metadata
    /// - Use dirty and move rectangles to optimize streaming applications
    /// - Move rectangles should be processed before dirty rectangles for correct visuals
    pub fn capture_frame_with_metadata(&mut self) -> CaptureFrameWithMetadataResult {
        let (surface, metadata) = self.capture_frame_to_surface_with_metadata()?;

        let mut rect = DXGI_MAPPED_RECT::default();
        unsafe { surface.Map(&mut rect, DXGI_MAP_READ)? };

        let desc = self
            .duplicated_output
            .as_ref()
            .ok_or(CaptureError::RefreshFailure)?
            .get_desc()?;
        let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as usize;
        let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as usize;

        let pitch = rect.Pitch as usize;
        let source = rect.pBits;

        let (rotated_width, rotated_height) = match desc.Rotation {
            DXGI_MODE_ROTATION_ROTATE90 | DXGI_MODE_ROTATION_ROTATE270 => (height, width),
            _ => (width, height),
        };

        let mut data_vec: Vec<BGRA8> = Vec::with_capacity(rotated_width * rotated_height);

        let bytes_per_pixel = mem::size_of::<BGRA8>();
        let source_slice = unsafe {
            slice::from_raw_parts(source as *const BGRA8, pitch * height / bytes_per_pixel)
        };

        match desc.Rotation {
            DXGI_MODE_ROTATION_IDENTITY | DXGI_MODE_ROTATION_UNSPECIFIED => {
                for i in 0..height {
                    let start = i * pitch / bytes_per_pixel;
                    let end = start + width;
                    data_vec.extend_from_slice(&source_slice[start..end]);
                }
            }
            DXGI_MODE_ROTATION_ROTATE90 => {
                for i in 0..width {
                    for j in (0..height).rev() {
                        let index = j * pitch / bytes_per_pixel + i;
                        data_vec.push(source_slice[index]);
                    }
                }
            }
            DXGI_MODE_ROTATION_ROTATE180 => {
                for i in (0..height).rev() {
                    for j in (0..width).rev() {
                        let index = i * pitch / bytes_per_pixel + j;
                        data_vec.push(source_slice[index]);
                    }
                }
            }
            DXGI_MODE_ROTATION_ROTATE270 => {
                for i in (0..width).rev() {
                    for j in 0..height {
                        let index = j * pitch / bytes_per_pixel + i;
                        data_vec.push(source_slice[index]);
                    }
                }
            }
            _ => {}
        }

        unsafe { surface.Unmap()? };

        Ok((data_vec, (rotated_width, rotated_height), metadata))
    }

    /// Captures a single frame and returns it as `Vec<u8>` along with frame metadata.
    ///
    /// This method captures the current screen content and returns it as a vector
    /// of raw bytes representing the pixel components along with comprehensive
    /// metadata about the frame.
    ///
    /// # Returns
    ///
    /// On success, returns `Ok((components, (width, height), metadata))` where:
    /// - `components` is a `Vec<u8>` containing the raw pixel component data
    /// - `width` and `height` are the frame dimensions in pixels
    /// - `metadata` is a [`FrameMetadata`] struct containing detailed frame information
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use dxgi_capture_rs::DXGIManager;
    ///
    /// let mut manager = DXGIManager::new(1000)?;
    ///
    /// match manager.capture_frame_components_with_metadata() {
    ///     Ok((components, (width, height), metadata)) => {
    ///         println!("Captured {}x{} frame with {} bytes", width, height, components.len());
    ///         
    ///         // Only process changed regions for efficiency
    ///         if metadata.has_updates() {
    ///             println!("Frame has {} total changes", metadata.total_change_count());
    ///             // Process components (4 bytes per pixel: B, G, R, A)
    ///         }
    ///     }
    ///     Err(e) => eprintln!("Capture failed: {:?}", e),
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn capture_frame_components_with_metadata(
        &mut self,
    ) -> CaptureFrameComponentsWithMetadataResult {
        let (surface, metadata) = self.capture_frame_to_surface_with_metadata()?;

        let mut rect = DXGI_MAPPED_RECT::default();
        unsafe { surface.Map(&mut rect, DXGI_MAP_READ)? };

        let desc = self
            .duplicated_output
            .as_ref()
            .ok_or(CaptureError::RefreshFailure)?
            .get_desc()?;
        let width = (desc.DesktopCoordinates.right - desc.DesktopCoordinates.left) as usize;
        let height = (desc.DesktopCoordinates.bottom - desc.DesktopCoordinates.top) as usize;

        let pitch = rect.Pitch as usize;
        let source = rect.pBits;

        let (rotated_width, rotated_height) = match desc.Rotation {
            DXGI_MODE_ROTATION_ROTATE90 | DXGI_MODE_ROTATION_ROTATE270 => (height, width),
            _ => (width, height),
        };

        let mut data_vec: Vec<u8> = Vec::with_capacity(rotated_width * rotated_height * 4);

        let bytes_per_pixel = 4; // BGRA
        let source_slice = unsafe { slice::from_raw_parts(source as *const u8, pitch * height) };

        match desc.Rotation {
            DXGI_MODE_ROTATION_IDENTITY | DXGI_MODE_ROTATION_UNSPECIFIED => {
                for i in 0..height {
                    let start = i * pitch;
                    let end = start + width * bytes_per_pixel;
                    data_vec.extend_from_slice(&source_slice[start..end]);
                }
            }
            DXGI_MODE_ROTATION_ROTATE90 => {
                for i in 0..width {
                    for j in (0..height).rev() {
                        let index = j * pitch + i * bytes_per_pixel;
                        data_vec.extend_from_slice(&source_slice[index..index + bytes_per_pixel]);
                    }
                }
            }
            DXGI_MODE_ROTATION_ROTATE180 => {
                for i in (0..height).rev() {
                    for j in (0..width).rev() {
                        let index = i * pitch + j * bytes_per_pixel;
                        data_vec.extend_from_slice(&source_slice[index..index + bytes_per_pixel]);
                    }
                }
            }
            DXGI_MODE_ROTATION_ROTATE270 => {
                for i in (0..width).rev() {
                    for j in 0..height {
                        let index = j * pitch + i * bytes_per_pixel;
                        data_vec.extend_from_slice(&source_slice[index..index + bytes_per_pixel]);
                    }
                }
            }
            _ => {}
        }

        unsafe { surface.Unmap()? };

        Ok((data_vec, (rotated_width, rotated_height), metadata))
    }
}

pub type CaptureFrameWithMetadataResult =
    Result<(Vec<BGRA8>, (usize, usize), FrameMetadata), CaptureError>;

pub type CaptureFrameComponentsWithMetadataResult =
    Result<(Vec<u8>, (usize, usize), FrameMetadata), CaptureError>;
