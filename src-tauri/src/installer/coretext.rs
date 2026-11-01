//! macOS CoreText FFI: kalıcı kullanıcı kapsamında kayıt, doğrulama ve sistem taraması.
use crate::error::{AppError, Result};
use block2::RcBlock;
use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFRelease, CFType, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::{CFURL, CFURLRef};
use std::collections::HashSet;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// kCTFontManagerScopePersistent: oturumlar arasında kalıcı, yalnız geçerli kullanıcı.
const SCOPE_PERSISTENT: u32 = 2;
const SCOPE_NONE: u32 = 0;
const ERROR_ALREADY_REGISTERED: isize = 105;
const ERROR_NOT_REGISTERED: isize = 201;

/// C `bool` tek bayttır; block2 `bool` kodlaması sağlamadığından `u8` kullanılır.
type Handler = block2::Block<dyn Fn(*const c_void, u8) -> u8>;

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    static kCTFontNameAttribute: CFStringRef;
    static kCTFontURLAttribute: CFStringRef;
    static kCTFontFamilyNameAttribute: CFStringRef;
    static kCTFontStyleNameAttribute: CFStringRef;

    fn CTFontManagerRegisterFontURLs(urls: CFArrayRef, scope: u32, enabled: bool, handler: *const Handler);
    fn CTFontManagerUnregisterFontURLs(urls: CFArrayRef, scope: u32, handler: *const Handler);
    fn CTFontManagerCreateFontDescriptorsFromURL(url: CFURLRef) -> CFArrayRef;
    fn CTFontManagerGetScopeForURL(url: CFURLRef) -> u32;
    fn CTFontManagerCopyAvailablePostScriptNames() -> CFArrayRef;
    fn CTFontDescriptorCopyAttribute(descriptor: CFTypeRef, attribute: CFStringRef) -> CFTypeRef;
    fn CTFontDescriptorCreateWithNameAndSize(name: CFStringRef, size: f64) -> CFTypeRef;
    fn CTFontCollectionCreateFromAvailableFonts(options: *const c_void) -> CFTypeRef;
    fn CTFontCollectionCreateMatchingFontDescriptors(collection: CFTypeRef) -> CFArrayRef;
}

extern "C" {
    fn CFErrorGetCode(error: CFTypeRef) -> isize;
    fn CFErrorCopyDescription(error: CFTypeRef) -> CFStringRef;
}

fn url_for(path: &Path) -> Result<CFURL> {
    CFURL::from_path(path, false).ok_or_else(|| AppError::Install(format!("geçersiz yol: {}", path.display())))
}

fn url_array(urls: &[CFURL]) -> CFArray<CFURL> {
    CFArray::from_CFTypes(urls)
}

#[derive(Debug)]
struct RegistrationError {
    code: isize,
    message: String,
}

/// CoreText'in zaman uyumsuz sonucunu bekler; işleyici birden çok kez çağrılabilir.
fn wait_for(call: impl FnOnce(&Handler)) -> Result<Vec<RegistrationError>> {
    let (tx, rx) = mpsc::channel::<(Vec<RegistrationError>, bool)>();
    let block = RcBlock::new(move |errors: *const c_void, done: u8| -> u8 {
        let mut found = Vec::new();
        if !errors.is_null() {
            let array = unsafe { CFArray::<CFType>::wrap_under_get_rule(errors.cast()) };
            for item in array.iter() {
                let raw = item.as_CFTypeRef();
                let code = unsafe { CFErrorGetCode(raw) };
                let message = unsafe { CFString::wrap_under_create_rule(CFErrorCopyDescription(raw)) }.to_string();
                found.push(RegistrationError { code, message });
            }
        }
        let _ = tx.send((found, done != 0));
        1
    });
    call(&block);
    let mut all = Vec::new();
    loop {
        let (errors, done) = rx
            .recv_timeout(Duration::from_secs(60))
            .map_err(|_| AppError::Install("CoreText yanıt vermedi".into()))?;
        all.extend(errors);
        if done {
            return Ok(all);
        }
    }
}

/// Dosyaları kalıcı olarak kaydeder. "Zaten kayıtlı" başarı sayılır: ~/Library/Fonts izlenir
/// ve sistem kopyalanan dosyayı kendiliğinden kaydedebilir. Asıl karar `is_registered` ile verilir.
pub fn register(paths: &[PathBuf]) -> Result<()> {
    let urls = paths.iter().map(|p| url_for(p)).collect::<Result<Vec<_>>>()?;
    let array = url_array(&urls);
    let errors = wait_for(|handler| unsafe {
        CTFontManagerRegisterFontURLs(array.as_concrete_TypeRef(), SCOPE_PERSISTENT, true, handler)
    })?;
    let fatal: Vec<_> = errors.iter().filter(|e| e.code != ERROR_ALREADY_REGISTERED).collect();
    match fatal.first() {
        Some(e) => Err(AppError::Install(format!("{} (kod {})", e.message, e.code))),
        None => Ok(()),
    }
}

pub fn unregister(paths: &[PathBuf]) -> Result<()> {
    let urls = paths.iter().map(|p| url_for(p)).collect::<Result<Vec<_>>>()?;
    let array = url_array(&urls);
    let errors = wait_for(|handler| unsafe {
        CTFontManagerUnregisterFontURLs(array.as_concrete_TypeRef(), SCOPE_PERSISTENT, handler)
    })?;
    let fatal: Vec<_> = errors.iter().filter(|e| e.code != ERROR_NOT_REGISTERED).collect();
    match fatal.first() {
        Some(e) => Err(AppError::Install(format!("{} (kod {})", e.message, e.code))),
        None => Ok(()),
    }
}

pub fn is_registered(path: &Path) -> bool {
    url_for(path)
        .map(|url| unsafe { CTFontManagerGetScopeForURL(url.as_concrete_TypeRef()) } != SCOPE_NONE)
        .unwrap_or(false)
}

/// CoreText'in dosyada gördüğü PostScript adları. Boşsa dosya macOS için geçersizdir.
pub fn describe(path: &Path) -> Result<Vec<String>> {
    let url = url_for(path)?;
    let raw = unsafe { CTFontManagerCreateFontDescriptorsFromURL(url.as_concrete_TypeRef()) };
    if raw.is_null() {
        return Ok(Vec::new());
    }
    let array = unsafe { CFArray::<CFType>::wrap_under_create_rule(raw) };
    Ok(array.iter().filter_map(|d| attribute_string(d.as_CFTypeRef(), unsafe { kCTFontNameAttribute })).collect())
}

fn attribute_string(descriptor: CFTypeRef, attribute: CFStringRef) -> Option<String> {
    let value = unsafe { CTFontDescriptorCopyAttribute(descriptor, attribute) };
    if value.is_null() {
        return None;
    }
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    value.downcast::<CFString>().map(|s| s.to_string())
}

fn attribute_path(descriptor: CFTypeRef) -> Option<PathBuf> {
    let value = unsafe { CTFontDescriptorCopyAttribute(descriptor, kCTFontURLAttribute) };
    if value.is_null() {
        return None;
    }
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    value.downcast::<CFURL>().and_then(|u| u.to_path())
}

pub fn available_postscript_names() -> HashSet<String> {
    let raw = unsafe { CTFontManagerCopyAvailablePostScriptNames() };
    if raw.is_null() {
        return HashSet::new();
    }
    let array = unsafe { CFArray::<CFType>::wrap_under_create_rule(raw) };
    array.iter().filter_map(|n| n.downcast::<CFString>().map(|s| s.to_string())).collect()
}

/// PostScript adının sistemde hangi dosyadan geldiğini bulur.
pub fn locate(postscript_name: &str) -> Option<PathBuf> {
    let name = CFString::new(postscript_name);
    let descriptor = unsafe { CTFontDescriptorCreateWithNameAndSize(name.as_concrete_TypeRef(), 12.0) };
    if descriptor.is_null() {
        return None;
    }
    let path = attribute_path(descriptor);
    unsafe { CFRelease(descriptor) };
    path
}

pub struct SystemFont {
    pub postscript_name: String,
    pub family: String,
    pub style: String,
    pub path: Option<PathBuf>,
}

pub fn scan_system_fonts() -> Vec<SystemFont> {
    let collection = unsafe { CTFontCollectionCreateFromAvailableFonts(std::ptr::null()) };
    if collection.is_null() {
        return Vec::new();
    }
    let raw = unsafe { CTFontCollectionCreateMatchingFontDescriptors(collection) };
    unsafe { CFRelease(collection) };
    if raw.is_null() {
        return Vec::new();
    }
    let array = unsafe { CFArray::<CFType>::wrap_under_create_rule(raw) };
    array
        .iter()
        .filter_map(|d| {
            let d = d.as_CFTypeRef();
            Some(SystemFont {
                postscript_name: attribute_string(d, unsafe { kCTFontNameAttribute })?,
                family: attribute_string(d, unsafe { kCTFontFamilyNameAttribute })?,
                style: attribute_string(d, unsafe { kCTFontStyleNameAttribute }).unwrap_or_default(),
                path: attribute_path(d),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sees_system_fonts_and_describes_files() {
        let names = available_postscript_names();
        assert!(names.contains("ArialMT"));
        assert_eq!(describe(Path::new("/System/Library/Fonts/Supplemental/Arial.ttf")).unwrap()[0], "ArialMT");
        assert!(locate("ArialMT").unwrap().to_string_lossy().contains("Arial"));
        assert!(scan_system_fonts().iter().any(|f| f.family == "Arial"));
    }

    #[test]
    fn invalid_file_has_no_descriptors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.ttf");
        std::fs::write(&path, b"nope").unwrap();
        assert!(describe(&path).unwrap().is_empty());
    }
}
