use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::Mutex;

use windows::Win32::Foundation::{DV_E_FORMATETC, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, E_NOTIMPL, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, PROPERTYKEY, S_OK};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
    DVASPECT_CONTENT, FORMATETC, IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA,
    STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, DoDragDrop, IDropSource, IDropSource_Impl,
    OleInitialize,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink};
use windows::core::{BOOL, GUID, HRESULT, Interface, PCWSTR, Result, implement};

use crate::state::Id;

const PKEY_TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0xF29F85E0_4FF9_1068_AB91_08002B27B3D9),
    pid: 2,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JumpItem {
    pub title: String,
    pub args: String,
    pub description: String,
}

pub fn jump_channel_args(guild: Id, ch: Id) -> String {
    if guild == 0 {
        format!("discord://-/channels/@me/{ch}")
    } else {
        format!("discord://-/channels/{guild}/{ch}")
    }
}

pub fn status_tasks() -> [JumpItem; 4] {
    [
        JumpItem { title: "Set Status: Online".into(), args: "discord://-/status/online".into(), description: "Switch status to Online".into() },
        JumpItem { title: "Set Status: Idle".into(), args: "discord://-/status/idle".into(), description: "Switch status to Idle".into() },
        JumpItem { title: "Set Status: Do Not Disturb".into(), args: "discord://-/status/dnd".into(), description: "Switch status to Do Not Disturb".into() },
        JumpItem { title: "Set Status: Invisible".into(), args: "discord://-/status/invisible".into(), description: "Switch status to Invisible".into() },
    ]
}

static LAST_JUMP: Mutex<Option<Vec<(String, String)>>> = Mutex::new(None);

pub fn update_jump_list(recent: Vec<JumpItem>) {
    if cfg!(test) || crate::winsys::harness() {
        return;
    }
    let sig: Vec<(String, String)> = recent.iter().map(|i| (i.title.clone(), i.args.clone())).collect();
    {
        let mut last = LAST_JUMP.lock().unwrap();
        if last.as_ref() == Some(&sig) {
            return;
        }
        *last = Some(sig);
    }
    let _ = std::thread::Builder::new().name("jumplist".into()).stack_size(256 << 10).spawn(move || {
        let _ = apply_jump_list(&recent);
    });
}

fn make_shell_link(exe: &str, item: &JumpItem) -> Result<IShellLinkW> {
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        let exe_w = crate::winsys::wide(exe);
        let args_w = crate::winsys::wide(&item.args);
        let desc_w = crate::winsys::wide(&item.description);
        link.SetPath(PCWSTR(exe_w.as_ptr()))?;
        link.SetArguments(PCWSTR(args_w.as_ptr()))?;
        link.SetDescription(PCWSTR(desc_w.as_ptr()))?;
        link.SetIconLocation(PCWSTR(exe_w.as_ptr()), 0)?;
        if let Ok(props) = link.cast::<IPropertyStore>() {
            let val = PROPVARIANT::from(item.title.as_str());
            let _ = props.SetValue(&PKEY_TITLE, &val);
            let _ = props.Commit();
        }
        Ok(link)
    }
}

fn apply_jump_list(recent: &[JumpItem]) -> Result<()> {
    let Ok(exe_path) = std::env::current_exe() else { return Ok(()) };
    let exe = exe_path.to_string_lossy().into_owned();
    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let need_uninit = hr.is_ok();
        let res = (|| -> Result<()> {
            let list: ICustomDestinationList = CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
            let app_id = crate::winsys::wide("rustcord");
            let _ = list.SetAppID(PCWSTR(app_id.as_ptr()));
            let mut slots = 0u32;
            let _removed: IObjectArray = list.BeginList(&mut slots)?;
            if !recent.is_empty() {
                let col: IObjectCollection = CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
                for item in recent.iter().take(slots.min(6) as usize) {
                    if let Ok(link) = make_shell_link(&exe, item) {
                        let _ = col.AddObject(&link);
                    }
                }
                let arr: IObjectArray = col.cast()?;
                let cat = crate::winsys::wide("Recent Channels");
                let _ = list.AppendCategory(PCWSTR(cat.as_ptr()), &arr);
            }
            let tasks: IObjectCollection = CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
            for item in status_tasks() {
                if let Ok(link) = make_shell_link(&exe, &item) {
                    let _ = tasks.AddObject(&link);
                }
            }
            let arr: IObjectArray = tasks.cast()?;
            let _ = list.AddUserTasks(&arr);
            list.CommitList()?;
            Ok(())
        })();
        if need_uninit {
            CoUninitialize();
        }
        res
    }
}

pub fn dropfiles_bytes(path: &Path) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + path.as_os_str().len() * 2 + 4);
    out.extend_from_slice(&20u32.to_le_bytes()); // pFiles
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    out.extend_from_slice(&0u32.to_le_bytes()); // fNC
    out.extend_from_slice(&1u32.to_le_bytes()); // fWide = TRUE
    for u in path.to_string_lossy().encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[implement(IDataObject)]
struct FileDataObject {
    bytes: Vec<u8>,
}

impl FileDataObject {
    fn accepts(fmt: *const FORMATETC) -> bool {
        if fmt.is_null() {
            return false;
        }
        let f = unsafe { &*fmt };
        f.cfFormat == CF_HDROP.0
            && (f.dwAspect == DVASPECT_CONTENT.0 || f.dwAspect == 0)
            && (f.tymed & TYMED_HGLOBAL.0 as u32) != 0
    }
}

impl IDataObject_Impl for FileDataObject_Impl {
    fn GetData(&self, pformatetcin: *const FORMATETC) -> Result<STGMEDIUM> {
        if !FileDataObject::accepts(pformatetcin) {
            return Err(DV_E_FORMATETC.into());
        }
        unsafe {
            let hmem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, self.bytes.len())?;
            let ptr = GlobalLock(hmem);
            if ptr.is_null() {
                return Err(DV_E_FORMATETC.into());
            }
            std::ptr::copy_nonoverlapping(self.bytes.as_ptr(), ptr.cast::<u8>(), self.bytes.len());
            let _ = GlobalUnlock(hmem);
            Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: hmem },
                pUnkForRelease: ManuallyDrop::new(None),
            })
        }
    }

    fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        if FileDataObject::accepts(pformatetc) { S_OK } else { DV_E_FORMATETC }
    }

    fn GetCanonicalFormatEtc(&self, _pformatectin: *const FORMATETC, pformatetcout: *mut FORMATETC) -> HRESULT {
        if !pformatetcout.is_null() {
            unsafe { (*pformatetcout).ptd = std::ptr::null_mut() };
        }
        HRESULT(0x00040130u32 as i32) // DATA_S_SAMEFORMATETC
    }

    fn SetData(&self, _pformatetc: *const FORMATETC, _pmedium: *const STGMEDIUM, _frelease: BOOL) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, _dwdirection: u32) -> Result<IEnumFORMATETC> {
        Err(E_NOTIMPL.into())
    }

    fn DAdvise(&self, _pformatetc: *const FORMATETC, _advf: u32, _padvsink: windows_core::Ref<'_, IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

#[implement(IDropSource)]
struct FileDropSource;

impl IDropSource_Impl for FileDropSource_Impl {
    fn QueryContinueDrag(&self, fescapepressed: BOOL, grfkeystate: MODIFIERKEYS_FLAGS) -> HRESULT {
        if fescapepressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if (grfkeystate.0 & MK_LBUTTON.0) == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _dweffect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

pub fn start_file_drag(path: &Path) -> bool {
    if cfg!(test) || crate::winsys::harness() || !path.exists() {
        return false;
    }
    let bytes = dropfiles_bytes(path);
    unsafe {
        let _ = OleInitialize(None);
        let data: IDataObject = FileDataObject { bytes }.into();
        let src: IDropSource = FileDropSource.into();
        let mut effect = DROPEFFECT_NONE;
        let hr = DoDragDrop(&data, &src, DROPEFFECT_COPY, &mut effect);
        hr.is_ok() && effect != DROPEFFECT_NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropfiles_header_and_utf16_payload_are_valid() {
        let buf = dropfiles_bytes(Path::new(r"C:\tmp\cat.png"));
        assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(buf[16..20].try_into().unwrap()), 1);
        let u16s: Vec<u16> = buf[20..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        assert!(u16s.ends_with(&[0, 0]));
        assert_eq!(String::from_utf16_lossy(&u16s[..u16s.len() - 2]), r"C:\tmp\cat.png");
    }

    #[test]
    fn jump_channel_args_formats_dm_and_guild_links() {
        assert_eq!(jump_channel_args(0, 42), "discord://-/channels/@me/42");
        assert_eq!(jump_channel_args(99, 42), "discord://-/channels/99/42");
    }
}
