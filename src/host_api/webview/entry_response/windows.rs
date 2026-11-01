//! Private WebView2 context and exact received-entry responses; no ambient file/network load.
use super::EntryResponseSource;
use std::{cell::RefCell, sync::Arc, task::Poll};
use tasty_remote::html_body::{NativeBodyResponse, ResponseWait};

pub(in crate::host_api::webview) struct PendingResponse {
    args: ICoreWebView2WebResourceRequestedEventArgs,
    deferral: ICoreWebView2Deferral,
    wait: ResponseWait,
}
use webview2_com::{
    CoreWebView2CustomSchemeRegistration, CoreWebView2EnvironmentOptions,
    Microsoft::Web::WebView2::Win32::*,
};
use windows::{
    Win32::{System::Com::CoTaskMemFree, UI::Shell::SHCreateMemStream},
    core::*,
};

pub(in crate::host_api::webview) struct EntryEnvironment {
    pub options: ICoreWebView2EnvironmentOptions,
    pub profile: tempfile::TempDir,
    pub folder: Vec<u16>,
}
impl EntryEnvironment {
    pub fn new() -> std::io::Result<Self> {
        let root = tasty_utils::path::tasty_home()
            .ok_or_else(|| std::io::Error::other("Tasty home is unavailable"))?
            .join("webview/entries");
        std::fs::create_dir_all(&root)?;
        let profile = tempfile::Builder::new().prefix("entry-").tempdir_in(root)?;
        use std::os::windows::ffi::OsStrExt;
        let folder = profile
            .path()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let options = CoreWebView2EnvironmentOptions::default();
        let scheme =
            CoreWebView2CustomSchemeRegistration::new(tasty_remote::html_body::ENTRY_SCHEME.into());
        // SAFETY: these freshly owned COM implementations have not escaped to another thread.
        #[allow(clippy::multiple_unsafe_ops_per_block)]
        unsafe {
            scheme.set_has_authority_component(true);
            options.set_scheme_registrations(vec![Some(scheme.into())]);
        }
        Ok(Self {
            options: options.into(),
            profile,
            folder,
        })
    }
}
pub(in crate::host_api::webview) fn reply(
    source: &EntryResponseSource,
    environment: &ICoreWebView2Environment,
    args: &ICoreWebView2WebResourceRequestedEventArgs,
    uri: &str,
    pending: &RefCell<Vec<PendingResponse>>,
) -> Result<()> {
    // SAFETY: request/response COM objects remain on the WebView2 apartment thread.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        let request = args.Request()?;
        let mut method = PWSTR::null();
        request.Method(&mut method)?;
        let is_get = method.to_string().is_ok_and(|method| method == "GET");
        CoTaskMemFree(Some(method.0.cast()));
        if !is_get || !source.allows_uri(uri) {
            return respond(source, environment, args, None);
        }
        if let Some(body) = source.response(uri) {
            return respond(source, environment, args, Some(body));
        }
        if pending.borrow().len() >= 256 {
            return respond(source, environment, args, None);
        }
        // GetDeferral precedes the waiter: a failed deferral cannot leave an orphan request.
        let deferral = args.GetDeferral()?;
        let Some(wait) = source.request(uri) else {
            let result = respond(source, environment, args, None);
            deferral.Complete()?;
            return result;
        };
        pending.borrow_mut().push(PendingResponse {
            args: args.clone(),
            deferral,
            wait,
        });
        Ok(())
    }
}
fn respond(
    source: &EntryResponseSource,
    environment: &ICoreWebView2Environment,
    args: &ICoreWebView2WebResourceRequestedEventArgs,
    body: Option<Arc<NativeBodyResponse>>,
) -> Result<()> {
    // SAFETY: owned COM request/response, used only on the WebView2 apartment thread.
    #[allow(clippy::multiple_unsafe_ops_per_block)]
    unsafe {
        let Some(body) =
            body.filter(|body| source.is_current() && body.bytes().len() <= u32::MAX as usize)
        else {
            let response = environment.CreateWebResourceResponse(
                None,
                403,
                w!("Blocked"),
                w!("Cache-Control: no-store\r\n"),
            )?;
            return args.SetResponse(&response);
        };
        // The memory stream owns its byte copy. It cannot be changed by a later Reload.
        let stream = SHCreateMemStream(Some(body.bytes()))
            .ok_or_else(|| Error::from_hresult(windows::Win32::Foundation::E_OUTOFMEMORY))?;
        let headers = format!(
            "Content-Type: {}\r\nContent-Length: {}\r\nContent-Security-Policy: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n",
            body.mime(),
            body.bytes().len(),
            source.csp()
        );
        let headers: Vec<u16> = headers.encode_utf16().chain(Some(0)).collect();
        let response = environment.CreateWebResourceResponse(
            &stream,
            200,
            w!("OK"),
            PCWSTR(headers.as_ptr()),
        )?;
        args.SetResponse(&response)
    }
}

/// Main-thread host synchronization drains completed resources; producer wakeups contain no COM.
pub(in crate::host_api::webview) fn poll(
    source: &EntryResponseSource,
    environment: &ICoreWebView2Environment,
    pending: &RefCell<Vec<PendingResponse>>,
) {
    let requests = std::mem::take(&mut *pending.borrow_mut());
    for mut request in requests {
        match source.poll_response(&mut request.wait) {
            Poll::Pending => pending.borrow_mut().push(request),
            Poll::Ready(result) => {
                if let Err(error) = respond(source, environment, &request.args, result.ok()) {
                    tracing::warn!(%error, "remote WebView2 response failed before deferral completion");
                }
                // Complete even if SetResponse failed, so native cannot retain a permanent hold.
                // SAFETY: owned deferral is completed once, on its originating apartment.
                if let Err(error) = unsafe { request.deferral.Complete() } {
                    tracing::warn!(%error, "remote WebView2 deferral completion failed");
                }
            }
        }
    }
}

/// Handle popups inside this context so they cannot create an unisolated default WebView.
pub(in crate::host_api::webview) fn deny_new_windows(view: &ICoreWebView2) -> Result<()> {
    let mut token = 0;
    let handler = webview2_com::NewWindowRequestedEventHandler::create(Box::new(|_, args| {
        if let Some(args) = args {
            // SAFETY: WebView2 calls the handler on its apartment with live args.
            unsafe {
                args.SetHandled(true)?;
            }
        }
        Ok(())
    }));
    // SAFETY: live WebView and callback on the COM apartment thread. WebView owns the callback.
    unsafe {
        view.add_NewWindowRequested(&handler, &mut token)?;
    }
    let external: ICoreWebView2_18 = view.cast()?;
    let handler =
        webview2_com::LaunchingExternalUriSchemeEventHandler::create(Box::new(|_, args| {
            if let Some(args) = args {
                // SAFETY: WebView2 supplied live event args on its apartment thread.
                unsafe {
                    args.SetCancel(true)?;
                }
            }
            Ok(())
        }));
    // SAFETY: live interface and handler on this COM apartment; the interface owns its callback.
    unsafe { external.add_LaunchingExternalUriScheme(&handler, &mut token) }
}
