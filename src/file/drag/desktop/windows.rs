//! Shell-provided IDataObject exposes CF_HDROP and Shell ID lists without touching the clipboard.
use super::{Outcome, Request};
use std::{
    cell::Cell,
    os::windows::ffi::OsStrExt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HWND, S_OK,
        },
        System::{
            Com::{CoTaskMemFree, IDataObject},
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DoDragDrop, IDropSource, IDropSource_Impl,
                IDropSourceNotify, IDropSourceNotify_Impl, OleInitialize, OleUninitialize,
            },
            SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS},
            Threading::GetCurrentThreadId,
        },
        UI::{
            Shell::{
                BHID_DataObject, Common::ITEMIDLIST, ILCreateFromPathW,
                SHCreateShellItemArrayFromIDLists,
            },
            WindowsAndMessaging::{
                GetWindowThreadProcessId, HCURSOR, IDC_NO, IsWindow, LoadCursorW, SetCursor,
            },
        },
    },
    core::{BOOL, HRESULT, PCWSTR, implement},
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[implement(IDropSource, IDropSourceNotify)]
struct Source {
    cancelled: Arc<AtomicBool>,
    own_target: Cell<bool>,
    no_drop: HCURSOR,
    window: HWND,
}
#[allow(non_snake_case)] // reason: COM interface method names are defined by the Windows SDK.
impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() || self.cancelled.load(Ordering::Acquire)
            // SAFETY: The request keeps the Rust owner alive; the native window can still be destroyed.
            || !unsafe { IsWindow(Some(self.window)) }.as_bool()
        {
            DRAGDROP_S_CANCEL
        } else if keys & MK_LBUTTON == MODIFIERKEYS_FLAGS(0) {
            if self.own_target.get() {
                DRAGDROP_S_CANCEL
            } else {
                DRAGDROP_S_DROP
            }
        } else {
            S_OK
        }
    }
    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        if self.own_target.get() {
            // SAFETY: A shared system cursor requires no bitmap or DestroyCursor ownership.
            unsafe {
                SetCursor(Some(self.no_drop));
            }
            S_OK
        } else {
            DRAGDROP_S_USEDEFAULTCURSORS
        }
    }
}
#[allow(non_snake_case)] // reason: COM interface method names are defined by the Windows SDK.
impl IDropSourceNotify_Impl for Source_Impl {
    fn DragEnterTarget(&self, target: HWND) -> windows::core::Result<()> {
        let mut pid = 0;
        // SAFETY: The handle is borrowed from OLE for this notification; no ownership is transferred.
        unsafe {
            GetWindowThreadProcessId(target, Some(&mut pid));
        }
        self.own_target.set(pid == std::process::id());
        Ok(())
    }
    fn DragLeaveTarget(&self) -> windows::core::Result<()> {
        self.own_target.set(false);
        Ok(())
    }
}
struct Ole;
impl Drop for Ole {
    fn drop(&mut self) {
        // SAFETY: created only after successful OleInitialize on this same UI thread.
        unsafe {
            OleUninitialize();
        }
    }
}
struct Pidl(*mut ITEMIDLIST);
impl Drop for Pidl {
    fn drop(&mut self) {
        // SAFETY: ILCreateFromPathW returns a COM-allocated PIDL owned by this guard.
        unsafe {
            CoTaskMemFree(Some(self.0.cast()));
        }
    }
}

pub(super) fn start(request: &Request) -> Result<Outcome, String> {
    let RawWindowHandle::Win32(handle) = request
        .window
        .window_handle()
        .map_err(|e| e.to_string())?
        .as_raw()
    else {
        return Err("OLE requires a Win32 window".into());
    };
    let hwnd = HWND(handle.hwnd.get() as *mut _);
    // SAFETY: IsWindow only inspects the handle value; the Arc<Window> in request keeps the
    // Rust owner alive for the whole drag, including OLE's nested message loop.
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err("drag source window is closed".into());
    }
    // SAFETY: hwnd was validated above and is only borrowed; no ownership is transferred.
    let window_thread = unsafe { GetWindowThreadProcessId(hwnd, None) };
    // SAFETY: GetCurrentThreadId has no preconditions.
    if window_thread != unsafe { GetCurrentThreadId() } {
        return Err("OLE drag must run on the source window UI thread".into());
    }
    // SAFETY: Called by App on the winit UI thread (checked above) after all View/engine guards
    // are released; the matching OleUninitialize runs in the Ole guard on this same thread.
    unsafe { OleInitialize(None) }.map_err(|e| e.to_string())?;
    let _ole = Ole;
    let mut pidls = Vec::with_capacity(request.paths.len());
    for path in &request.paths {
        let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: wide is NUL-terminated and outlives the call; the returned PIDL is owned by Pidl.
        let pidl = unsafe { ILCreateFromPathW(PCWSTR(wide.as_ptr())) };
        if pidl.is_null() {
            return Err("Shell could not resolve a drag path".into());
        }
        pidls.push(Pidl(pidl));
    }
    let pointers: Vec<*const ITEMIDLIST> = pidls.iter().map(|p| p.0.cast_const()).collect();
    // SAFETY: Every pointer is a live PIDL held by pidls, which outlives the shell item array use.
    let items =
        unsafe { SHCreateShellItemArrayFromIDLists(&pointers) }.map_err(|e| e.to_string())?;
    // SAFETY: items is a valid IShellItemArray and BHID_DataObject is a static GUID.
    let data: IDataObject =
        unsafe { items.BindToHandler(None, &BHID_DataObject) }.map_err(|e| e.to_string())?;
    // SAFETY: A shared system cursor requires no bitmap or DestroyCursor ownership.
    let no_drop = unsafe { LoadCursorW(None, IDC_NO) }.map_err(|e| e.to_string())?;
    let source: IDropSource = Source {
        cancelled: request.cancelled.clone(),
        own_target: Cell::new(false),
        no_drop,
        window: hwnd,
    }
    .into();
    let mut effect = DROPEFFECT::default();
    crate::host_api::webview::entering_nested_message_loop();
    // SAFETY: OLE is initialized on this UI thread by _ole; data and source are live COM objects,
    // and effect outlives the modal drag loop.
    let status = unsafe { DoDragDrop(&data, &source, DROPEFFECT_COPY, &mut effect) };
    if status == DRAGDROP_S_CANCEL {
        Ok(Outcome::Cancelled)
    } else if status == DRAGDROP_S_DROP && effect == DROPEFFECT_COPY {
        Ok(Outcome::Copied)
    } else if status.is_err() {
        Err(format!("DoDragDrop failed: {status:?}"))
    } else {
        Ok(Outcome::Unknown)
    }
}
