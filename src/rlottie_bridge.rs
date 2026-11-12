use libloading::{Library, Symbol};
use std::ffi::{c_char, c_void, CString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Debug)]
pub enum RLottieError {
    LibraryLoadError(String),
    SymbolError(String),
    NullAnimationHandle,
    CStringError,
}

impl std::fmt::Display for RLottieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RLottieError::LibraryLoadError(e) => write!(f, "Failed to load rlottie.dll: {}", e),
            RLottieError::SymbolError(e) => write!(f, "Failed to resolve rlottie symbol: {}", e),
            RLottieError::NullAnimationHandle => write!(f, "lottie_animation_from_data returned NULL"),
            RLottieError::CStringError => write!(f, "JSON string contained invalid null bytes"),
        }
    }
}

impl std::error::Error for RLottieError {}

type FnFromData = unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> *mut c_void;
type FnDestroy = unsafe extern "C" fn(*mut c_void);
type FnDimensions = unsafe extern "C" fn(*mut c_void, *mut usize, *mut usize);
type FnTotalFrame = unsafe extern "C" fn(*mut c_void) -> usize;
type FnFrameRate = unsafe extern "C" fn(*mut c_void) -> f64;
type FnRender = unsafe extern "C" fn(*mut c_void, usize, *mut u32, usize, usize, usize);
type FnInit = unsafe extern "C" fn();
type FnConfigureCache = unsafe extern "C" fn(usize);

pub struct RLottieLib {
    _lib: Library,
    from_data: FnFromData,
    destroy: FnDestroy,
    dimensions: FnDimensions,
    total_frame: FnTotalFrame,
    frame_rate: FnFrameRate,
    render: FnRender,
}

// Global cached library instance so rlottie.dll is only loaded once per process
static GLOBAL_LIB: OnceLock<Mutex<Option<Arc<RLottieLib>>>> = OnceLock::new();

impl RLottieLib {
    pub fn get_or_load(path: &Path) -> Result<Arc<Self>, RLottieError> {
        let mutex = GLOBAL_LIB.get_or_init(|| Mutex::new(None));
        let mut guard = mutex.lock().map_err(|e| RLottieError::LibraryLoadError(e.to_string()))?;

        if let Some(ref lib) = *guard {
            return Ok(Arc::clone(lib));
        }

        let canonical_path: PathBuf = if path.exists() {
            path.to_path_buf()
        } else if Path::new("bin/rlottie.dll").exists() {
            PathBuf::from("bin/rlottie.dll")
        } else if Path::new("rust_bot/bin/rlottie.dll").exists() {
            PathBuf::from("rust_bot/bin/rlottie.dll")
        } else if Path::new("bin/librlottie.so").exists() {
            PathBuf::from("bin/librlottie.so")
        } else if Path::new("rust_bot/bin/librlottie.so").exists() {
            PathBuf::from("rust_bot/bin/librlottie.so")
        } else {
            path.to_path_buf()
        };

        unsafe {
            let lib = Library::new(&canonical_path)
                .map_err(|e| RLottieError::LibraryLoadError(format!("{:?}: {}", canonical_path, e)))?;

            let from_data_sym: Symbol<FnFromData> = lib
                .get(b"lottie_animation_from_data\0")
                .map_err(|e| RLottieError::SymbolError(e.to_string()))?;
            let destroy_sym: Symbol<FnDestroy> = lib
                .get(b"lottie_animation_destroy\0")
                .map_err(|e| RLottieError::SymbolError(e.to_string()))?;
            let dimensions_sym: Symbol<FnDimensions> = lib
                .get(b"lottie_animation_get_size\0")
                .map_err(|e| RLottieError::SymbolError(format!("lottie_animation_get_size: {}", e)))?;
            let total_frame_sym: Symbol<FnTotalFrame> = lib
                .get(b"lottie_animation_get_totalframe\0")
                .map_err(|e| RLottieError::SymbolError(e.to_string()))?;
            let frame_rate_sym: Symbol<FnFrameRate> = lib
                .get(b"lottie_animation_get_framerate\0")
                .map_err(|e| RLottieError::SymbolError(e.to_string()))?;
            let render_sym: Symbol<FnRender> = lib
                .get(b"lottie_animation_render\0")
                .map_err(|e| RLottieError::SymbolError(e.to_string()))?;

            if let Ok(init_sym) = lib.get::<FnInit>(b"lottie_init\0") {
                init_sym();
            }

            if let Ok(cfg_sym) = lib.get::<FnConfigureCache>(b"lottie_configure_model_cache_size\0") {
                cfg_sym(0);
            }

            let rlottie_lib = Arc::new(RLottieLib {
                from_data: *from_data_sym,
                destroy: *destroy_sym,
                dimensions: *dimensions_sym,
                total_frame: *total_frame_sym,
                frame_rate: *frame_rate_sym,
                render: *render_sym,
                _lib: lib,
            });

            *guard = Some(Arc::clone(&rlottie_lib));
            Ok(rlottie_lib)
        }
    }
}

pub struct RLottieAnimation {
    handle: *mut c_void,
    lib: Arc<RLottieLib>,
}

unsafe impl Send for RLottieAnimation {}

impl RLottieAnimation {
    pub fn from_json(json: &str, dll_path: &Path) -> Result<Self, RLottieError> {
        let lib = RLottieLib::get_or_load(dll_path)?;
        let c_json = CString::new(json).map_err(|_| RLottieError::CStringError)?;
        let unique_key = uuid::Uuid::new_v4().to_string();
        let c_key = CString::new(unique_key).map_err(|_| RLottieError::CStringError)?;
        let c_res = CString::new("").map_err(|_| RLottieError::CStringError)?;

        let handle = unsafe { (lib.from_data)(c_json.as_ptr(), c_key.as_ptr(), c_res.as_ptr()) };

        if handle.is_null() {
            return Err(RLottieError::NullAnimationHandle);
        }

        Ok(RLottieAnimation { handle, lib })
    }

    pub fn total_frames(&self) -> usize {
        unsafe { (self.lib.total_frame)(self.handle) }
    }

    pub fn frame_rate(&self) -> f64 {
        unsafe { (self.lib.frame_rate)(self.handle) }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        let mut width: usize = 0;
        let mut height: usize = 0;
        unsafe {
            (self.lib.dimensions)(self.handle, &mut width, &mut height);
        }
        (width, height)
    }

    pub fn render_frame(
        &self,
        frame: usize,
        buffer: &mut [u32],
        width: usize,
        height: usize,
        stride: usize,
    ) {
        assert!(
            buffer.len() >= width * height,
            "Buffer size ({}) is smaller than required frame size ({})",
            buffer.len(),
            width * height
        );
        unsafe {
            (self.lib.render)(
                self.handle,
                frame,
                buffer.as_mut_ptr(),
                width,
                height,
                stride,
            );
        }
    }
}

impl Drop for RLottieAnimation {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe {
                (self.lib.destroy)(self.handle);
            }
            self.handle = std::ptr::null_mut();
        }
    }
}
