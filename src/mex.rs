//! Direct MEX entry point. It passes UTF-8 JSON bytes to the C ABI without a C++ compiler.
use super::{omnibci_call, omnibci_free};
use std::{
    ffi::{CStr, c_void},
    ptr,
};

#[repr(C)]
pub struct MxArray {
    _opaque: [u8; 0],
}

#[cfg_attr(target_os = "windows", link(name = "libmx"))]
#[cfg_attr(not(target_os = "windows"), link(name = "mx"))]
unsafe extern "C" {
    fn mxGetClassID(array: *const MxArray) -> i32;
    fn mxGetNumberOfElements(array: *const MxArray) -> usize;
    fn mxGetData(array: *const MxArray) -> *mut c_void;
    // macOS libmx exports the R2018a C Matrix API entry point with this suffix.
    #[cfg_attr(target_os = "macos", link_name = "mxCreateNumericMatrix_800")]
    fn mxCreateNumericMatrix(
        rows: usize,
        cols: usize,
        class_id: i32,
        complexity: i32,
    ) -> *mut MxArray;
}

const MX_UINT8_CLASS: i32 = 9;
const MX_REAL: i32 = 0;

/// SDK shutdown is bounded and may leave an OS BLE close running. On Windows,
/// retain this DLL until process termination so `clear mex` cannot unmap code
/// underneath those threads or WinRT callbacks. Explicit Board.close still
/// releases device ownership; replacing the binary requires restarting MATLAB.
#[cfg(target_os = "windows")]
pub(super) fn retain_for_background_work() -> Result<(), String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleExW(flags: u32, address: *const u16, module: *mut *mut c_void) -> i32;
    }
    static RETAINED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    RETAINED
        .get_or_init(|| {
            let mut module = ptr::null_mut();
            // GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS.
            // SAFETY: the function address belongs to this loaded DLL; module is
            // a writable output pointer. PIN intentionally has no FreeLibrary.
            let success = unsafe {
                GetModuleHandleExW(
                    0x1 | 0x4,
                    retain_for_background_work as *const () as *const u16,
                    &mut module,
                )
            };
            if success == 0 {
                Err(format!(
                    "Cannot retain the MEX for background device work: {}",
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(())
            }
        })
        .clone()
}

unsafe fn response(bytes: &[u8]) -> *mut MxArray {
    // SAFETY: MATLAB owns the returned mxArray; both data pointers are valid for the duration of the copy.
    let result = unsafe { mxCreateNumericMatrix(1, bytes.len(), MX_UINT8_CLASS, MX_REAL) };
    if !result.is_null() && !bytes.is_empty() {
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), mxGetData(result).cast::<u8>(), bytes.len());
        }
    }
    result
}

/// MATLAB's MEX entry point. Input and output are uint8 vectors containing UTF-8 JSON.
/// # Safety
/// MATLAB supplies valid mxArray pointers and an output slot when `nlhs == 1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mexFunction(
    nlhs: i32,
    plhs: *mut *mut MxArray,
    nrhs: i32,
    prhs: *const *const MxArray,
) {
    if nlhs != 1 || plhs.is_null() {
        return;
    }
    let error = br#"{"ok":false,"error":"expected one uint8 JSON argument"}"#;
    if nrhs != 1 || prhs.is_null() {
        unsafe {
            *plhs = response(error);
        }
        return;
    }
    let input = unsafe { *prhs };
    if input.is_null() || unsafe { mxGetClassID(input) } != MX_UINT8_CLASS {
        unsafe {
            *plhs = response(error);
        }
        return;
    }
    let length = unsafe { mxGetNumberOfElements(input) };
    let data = unsafe { mxGetData(input) }.cast::<u8>();
    if data.is_null() {
        unsafe {
            *plhs = response(error);
        }
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, length) };
    let mut request = Vec::with_capacity(length + 1);
    request.extend_from_slice(bytes);
    request.push(0);
    let answer = unsafe { omnibci_call(request.as_ptr().cast()) };
    let answer_bytes = if answer.is_null() {
        error.to_vec()
    } else {
        // SAFETY: omnibci_call returns a NUL-terminated owned string.
        let text = unsafe { CStr::from_ptr(answer) }.to_bytes().to_vec();
        unsafe {
            omnibci_free(answer);
        }
        text
    };
    unsafe {
        *plhs = response(&answer_bytes);
    }
}
