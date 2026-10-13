//! Bounded local image decoding. Markdown cannot fetch remote resources.
fn image_signature(path: &std::path::Path) -> Option<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{Direct2D::*, Imaging::*},
        System::Com::*,
    },
    core::{PCWSTR, Result},
};
pub struct Images {
    factory: IWICImagingFactory,
    cache: HashMap<PathBuf, Option<ID2D1Bitmap>>,
    signatures: HashMap<PathBuf, Option<(u64, std::time::SystemTime)>>,
}
impl Images {
    pub fn new() -> Result<Self> {
        unsafe {
            Ok(Self {
                factory: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
                cache: HashMap::new(),
                signatures: HashMap::new(),
            })
        }
    }
    pub fn clear(&mut self) {
        self.cache.clear();
        self.signatures.clear();
    }
    pub fn refresh_changed(&mut self) -> bool {
        let changed: Vec<_> = self
            .signatures
            .iter()
            .filter(|(path, signature)| image_signature(path) != **signature)
            .map(|(path, _)| path.clone())
            .collect();
        for path in &changed {
            self.cache.remove(path);
            self.signatures.remove(path);
        }
        !changed.is_empty()
    }
    pub fn load(
        &mut self,
        target: &ID2D1HwndRenderTarget,
        directory: &Path,
        source: &str,
    ) -> Option<ID2D1Bitmap> {
        if source.contains(':') || source.starts_with(['\\', '/']) {
            return None;
        }
        let root = directory.canonicalize().ok()?;
        let path = root.join(source).canonicalize().ok()?;
        if !path.starts_with(&root) || std::fs::metadata(&path).ok()?.len() > 8 * 1024 * 1024 {
            return None;
        }
        if let Some(bitmap) = self.cache.get(&path) {
            return bitmap.clone();
        }
        if self.cache.len() >= 8 {
            return None;
        }
        let bitmap = (|| -> Result<ID2D1Bitmap> {
            unsafe {
                let wide: Vec<u16> = path
                    .as_os_str()
                    .to_string_lossy()
                    .encode_utf16()
                    .chain([0])
                    .collect();
                let decoder = self.factory.CreateDecoderFromFilename(
                    PCWSTR(wide.as_ptr()),
                    None,
                    GENERIC_READ,
                    WICDecodeMetadataCacheOnLoad,
                )?;
                let frame = decoder.GetFrame(0)?;
                let (mut w, mut h) = (0, 0);
                frame.GetSize(&mut w, &mut h)?;
                if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 4_000_000 {
                    return Err(windows::core::Error::new(
                        E_FAIL,
                        "Image exceeds pixel limit",
                    ));
                }
                let converter = self.factory.CreateFormatConverter()?;
                converter.Initialize(
                    &frame,
                    &GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )?;
                target.CreateBitmapFromWicBitmap(&converter, None)
            }
        })()
        .ok();
        self.signatures.insert(path.clone(), image_signature(&path));
        self.cache.insert(path, bitmap.clone());
        bitmap
    }
}
