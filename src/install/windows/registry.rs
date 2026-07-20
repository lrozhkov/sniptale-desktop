//! Windows registry string-value reader for install registration facts.
#![cfg(windows)]
#![allow(unsafe_code)]

use windows::{
    Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ, REG_VALUE_TYPE,
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW,
        },
    },
    core::PCWSTR,
};

use super::super::registration_facts::RegistryHive;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsRegistryError {
    OpenFailed,
    QueryFailed,
    UnsupportedType,
    InvalidUtf16,
}

pub(crate) fn read_string_value(
    hive: RegistryHive,
    subkey: &str,
    value_name: Option<&str>,
) -> Result<Option<String>, WindowsRegistryError> {
    let key = match RegistryKey::open(hive, subkey)? {
        Some(key) => key,
        None => return Ok(None),
    };
    key.read_string_value(value_name)
}

#[derive(Debug)]
struct RegistryKey {
    handle: HKEY,
}

impl RegistryKey {
    fn open(hive: RegistryHive, subkey: &str) -> Result<Option<Self>, WindowsRegistryError> {
        let subkey = wide_null(subkey);
        let mut handle = HKEY::default();
        // SAFETY: `subkey` is NUL-terminated and remains alive for the duration of the call;
        // RegOpenKeyExW writes one owned HKEY into `handle`, wrapped immediately for RegCloseKey.
        let status = unsafe {
            RegOpenKeyExW(
                root_key(hive),
                PCWSTR(subkey.as_ptr()),
                Some(0),
                KEY_READ,
                &mut handle,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(WindowsRegistryError::OpenFailed);
        }
        Ok(Some(Self { handle }))
    }

    fn read_string_value(
        &self,
        value_name: Option<&str>,
    ) -> Result<Option<String>, WindowsRegistryError> {
        let value_name = value_name.map(wide_null);
        let value_name = value_name
            .as_ref()
            .map_or(PCWSTR::null(), |name| PCWSTR(name.as_ptr()));
        let mut value_type = REG_VALUE_TYPE(0);
        let mut byte_len = 0_u32;
        // SAFETY: this first query passes no output buffer to obtain type and required byte size.
        let status = unsafe {
            RegQueryValueExW(
                self.handle,
                value_name,
                None,
                Some(&mut value_type),
                None,
                Some(&mut byte_len),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(WindowsRegistryError::QueryFailed);
        }
        if value_type != REG_SZ {
            return Err(WindowsRegistryError::UnsupportedType);
        }
        if byte_len == 0 {
            return Ok(Some(String::new()));
        }

        let wide_len = byte_len.div_ceil(2) as usize;
        let mut buffer = vec![0_u16; wide_len];
        // SAFETY: `buffer` has enough storage for `byte_len` bytes reported by the first query;
        // the API writes UTF-16 REG_SZ data into the buffer before this function decodes it.
        let status = unsafe {
            RegQueryValueExW(
                self.handle,
                value_name,
                None,
                Some(&mut value_type),
                Some(buffer.as_mut_ptr().cast::<u8>()),
                Some(&mut byte_len),
            )
        };
        if status != ERROR_SUCCESS {
            return Err(WindowsRegistryError::QueryFailed);
        }
        let used_len = (byte_len / 2) as usize;
        let used = buffer.get(..used_len).unwrap_or(&buffer);
        let used = used.strip_suffix(&[0]).unwrap_or(used);
        String::from_utf16(used)
            .map_err(|_| WindowsRegistryError::InvalidUtf16)
            .map(Some)
    }
}

impl Drop for RegistryKey {
    fn drop(&mut self) {
        if self.handle != HKEY::default() {
            // SAFETY: the opened registry key handle is owned by this RAII wrapper and closed once.
            let _ = unsafe { RegCloseKey(self.handle) };
            self.handle = HKEY::default();
        }
    }
}

const fn root_key(hive: RegistryHive) -> HKEY {
    match hive {
        RegistryHive::CurrentUser => HKEY_CURRENT_USER,
        RegistryHive::LocalMachine => HKEY_LOCAL_MACHINE,
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
