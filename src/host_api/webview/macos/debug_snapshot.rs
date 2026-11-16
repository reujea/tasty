//! debug 빌드에서 WebView 자체 그림과 문서 계산값을 파일로 남긴다(`debug.webview.snapshot`).
//! 화면 기록 권한 없이 WKWebView `takeSnapshotWithConfiguration:`으로 찍으므로 WebView 밖의 호스트
//! 그림(chrome·띠)은 담기지 않는다. 문서 값은 제품 경로와 같은 page world의 `evaluateJavaScript`로
//! 읽으므로, 페이지 스크립트가 꺼진 문서에서 실패하면 제품 스크립트도 같은 조건에 있다.

use std::path::{Path, PathBuf};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSError, NSString};

/// 읽는 값. 계약 속성의 계산값과 인라인 선언, 문서 바탕, 스크롤 위치다.
const PROBE: &str = "(()=>{const r=document.documentElement;\
if(!r)return JSON.stringify({root:false});\
const c=getComputedStyle(r);\
const b=document.body?getComputedStyle(document.body).backgroundColor:null;\
return JSON.stringify({root:true,\
bed_focused:c.getPropertyValue('--tasty-bed-focused'),\
inline_style:r.getAttribute('style'),\
md_bg:c.getPropertyValue('--md-bg'),\
md_row:c.getPropertyValue('--md-row'),\
md_zebra:c.getPropertyValue('--md-zebra'),\
body_bg:b,scroll_y:window.scrollY,ready:document.readyState});})()";

/// NSBitmapImageFileType.png
const PNG_FILE_TYPE: usize = 4;

impl super::PlatformWebView {
    /// `<base>.png`(그림)와 `<base>.json`(문서 값)을 비동기로 쓴다. 완료는 파일이 생긴 것으로 판정한다.
    pub fn debug_snapshot(&self, base: &Path) {
        let png = with_extension(base, "png");
        let json = with_extension(base, "json");
        let shot = RcBlock::new(move |image: *mut AnyObject, err: *mut NSError| {
            // SAFETY: WebKit이 main thread에서 완료를 부르며 두 포인터는 그동안 유효하거나 null이다.
            let outcome = match (unsafe { image.as_ref() }, unsafe { err.as_ref() }) {
                (Some(image), _) => write_png(image, &png),
                (None, Some(err)) => Err(err.localizedDescription().to_string()),
                (None, None) => Err("snapshot returned no image".to_string()),
            };
            if let Err(e) = outcome {
                tracing::warn!(path = %png.display(), "debug webview snapshot failed: {e}");
            }
        });
        // SAFETY: main thread WKWebView API. 설정이 nil이면 보이는 영역 전체를 찍는다.
        // WebKit이 블록을 복사해 완료 때 한 번 부른다.
        unsafe {
            let _: () = msg_send![
                &*self.webview,
                takeSnapshotWithConfiguration: std::ptr::null::<AnyObject>(),
                completionHandler: &*shot
            ];
        }
        let probe = RcBlock::new(move |result: *mut AnyObject, err: *mut NSError| {
            // SAFETY: 위와 같다.
            let body = match (unsafe { result.as_ref() }, unsafe { err.as_ref() }) {
                (Some(value), _) => match value.downcast_ref::<NSString>() {
                    Some(text) => format!("{{\"ok\":true,\"value\":{text}}}"),
                    None => "{\"ok\":false,\"error\":\"result is not a string\"}".to_string(),
                },
                (None, Some(err)) => serde_json::json!({
                    "ok": false,
                    "error": err.localizedDescription().to_string(),
                })
                .to_string(),
                (None, None) => "{\"ok\":false,\"error\":\"no result\"}".to_string(),
            };
            if let Err(e) = std::fs::write(&json, body) {
                tracing::warn!(path = %json.display(), "debug webview probe write failed: {e}");
            }
        });
        let source = NSString::from_str(PROBE);
        // SAFETY: main thread WKWebView API. WebKit이 블록을 복사해 완료 때 한 번 부른다.
        unsafe {
            self.webview
                .evaluateJavaScript_completionHandler(&source, Some(&probe));
        }
    }
}

fn with_extension(base: &Path, ext: &str) -> PathBuf {
    let mut name = base.as_os_str().to_owned();
    name.push(".");
    name.push(ext);
    PathBuf::from(name)
}

/// NSImage를 TIFF → NSBitmapImageRep → PNG로 바꿔 쓴다.
fn write_png(image: &AnyObject, path: &Path) -> Result<(), String> {
    // SAFETY: main thread AppKit 호출. 각 반환값은 autorelease 객체를 retain한 것이다.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        let tiff: Option<Retained<AnyObject>> = msg_send![image, TIFFRepresentation];
        let tiff = tiff.ok_or("image has no TIFF representation")?;
        let rep: Option<Retained<AnyObject>> =
            msg_send![class!(NSBitmapImageRep), imageRepWithData: &*tiff];
        let rep = rep.ok_or("TIFF data has no bitmap representation")?;
        let props: Retained<AnyObject> = msg_send![class!(NSDictionary), dictionary];
        let data: Option<Retained<AnyObject>> = msg_send![
            &*rep,
            representationUsingType: PNG_FILE_TYPE,
            properties: &*props
        ];
        let data = data.ok_or("bitmap has no PNG representation")?;
        let target = NSString::from_str(&path.to_string_lossy());
        let ok: bool = msg_send![&*data, writeToFile: &*target, atomically: true];
        if ok {
            Ok(())
        } else {
            Err("writeToFile failed".to_string())
        }
    }
}
