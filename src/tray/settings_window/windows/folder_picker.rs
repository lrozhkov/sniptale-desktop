//! Typed `IFileOpenDialog` adapter for selecting a Manual destination.
#![allow(unsafe_code)]

use std::{os::windows::ffi::OsStringExt, path::PathBuf};

use windows::{
    Win32::{
        Foundation::{HWND, S_FALSE},
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
            CoTaskMemFree, CoUninitialize,
        },
        UI::Shell::{FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH},
    },
    core::{HRESULT, PWSTR},
};

const HRESULT_CANCELLED: HRESULT = HRESULT(0x8007_04C7_u32 as i32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FolderPickerError {
    Apartment,
    Dialog,
    Result,
    Path,
}

pub(crate) fn pick_folder(owner: Option<HWND>) -> Result<Option<PathBuf>, FolderPickerError> {
    let _apartment = ComApartment::initialize()?;
    let dialog = create_dialog()?;
    // SAFETY: `dialog` is a live COM interface created in the initialized current apartment.
    let options = unsafe { dialog.GetOptions() }.map_err(|_| FolderPickerError::Dialog)?;
    // SAFETY: updates options on that live dialog before it is shown.
    unsafe { dialog.SetOptions(options | FOS_PICKFOLDERS) }
        .map_err(|_| FolderPickerError::Dialog)?;
    // SAFETY: when present, `owner` belongs to the modeless settings adapter and stays valid
    // through Show. First-time Manual setup intentionally has no settings HWND yet; IFileDialog
    // supports an absent owner for that user-initiated modal selection.
    match unsafe { dialog.Show(owner) } {
        Ok(()) => selected_path(&dialog).map(Some),
        Err(error) if error.code() == HRESULT_CANCELLED => Ok(None),
        Err(_) => Err(FolderPickerError::Dialog),
    }
}

#[derive(Debug)]
struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, FolderPickerError> {
        // SAFETY: initializes COM for this thread; the RAII guard balances successful calls.
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result == S_FALSE {
            Ok(Self)
        } else {
            result
                .ok()
                .map(|()| Self)
                .map_err(|_| FolderPickerError::Apartment)
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: the guard is constructed only after a successful COM initialization result.
        unsafe { CoUninitialize() };
    }
}

fn create_dialog() -> Result<IFileOpenDialog, FolderPickerError> {
    // SAFETY: creates the in-process file dialog after the current thread COM initialization.
    unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
        .map_err(|_| FolderPickerError::Dialog)
}

fn selected_path(dialog: &IFileOpenDialog) -> Result<PathBuf, FolderPickerError> {
    // SAFETY: the dialog completed successfully and owns the result item until this call returns.
    let item = unsafe { dialog.GetResult() }.map_err(|_| FolderPickerError::Result)?;
    // SAFETY: the live shell item allocates the returned display name with CoTaskMemAlloc.
    let value =
        unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }.map_err(|_| FolderPickerError::Path)?;
    pwstr_path(value)
}

fn pwstr_path(value: PWSTR) -> Result<PathBuf, FolderPickerError> {
    if value.is_null() {
        return Err(FolderPickerError::Path);
    }
    let allocation = ShellPath(value);
    let mut length = 0_usize;
    // SAFETY: `value` is a NUL-terminated shell-allocated UTF-16 path for this call.
    unsafe {
        while *allocation.0.0.add(length) != 0 {
            length = length.checked_add(1).ok_or(FolderPickerError::Path)?;
        }
    }
    // SAFETY: the bounded scan above proves this slice covers the allocated UTF-16 string.
    let path = unsafe { std::slice::from_raw_parts(allocation.0.0, length) };
    let result = PathBuf::from(std::ffi::OsString::from_wide(path));
    if result.as_os_str().is_empty() {
        Err(FolderPickerError::Path)
    } else {
        Ok(result)
    }
}

#[derive(Debug)]
struct ShellPath(PWSTR);

impl Drop for ShellPath {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns exactly the shell allocation returned by GetDisplayName.
        unsafe { CoTaskMemFree(Some(self.0.0.cast())) };
    }
}
