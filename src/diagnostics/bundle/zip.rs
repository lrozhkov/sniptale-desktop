//! Minimal stored ZIP writer for diagnostics bundles.

use std::{
    fs::File,
    io::{self, Seek, Write},
    path::Path,
};

const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x0403_4b50;
const CENTRAL_DIRECTORY_HEADER_SIGNATURE: u32 = 0x0201_4b50;
const END_OF_CENTRAL_DIRECTORY_SIGNATURE: u32 = 0x0605_4b50;
const VERSION_NEEDED: u16 = 20;
const STORED_METHOD: u16 = 0;

pub(super) struct ZipEntryInput<'a> {
    pub(super) name: &'a str,
    pub(super) content: &'a [u8],
}

struct CentralDirectoryEntry {
    name: Vec<u8>,
    crc32: u32,
    size: u32,
    local_header_offset: u32,
}

pub(super) fn write_zip_file(path: &Path, entries: &[ZipEntryInput<'_>]) -> io::Result<()> {
    let mut file = File::create(path)?;
    let mut central_directory = Vec::with_capacity(entries.len());
    for entry in entries {
        central_directory.push(write_local_entry(&mut file, entry)?);
    }

    let central_directory_offset = u32_from_u64(file.stream_position()?)?;
    for entry in &central_directory {
        write_central_directory_entry(&mut file, entry)?;
    }
    let central_directory_end = u32_from_u64(file.stream_position()?)?;
    let central_directory_size = central_directory_end
        .checked_sub(central_directory_offset)
        .ok_or_else(invalid_zip_size)?;
    write_end_of_central_directory(
        &mut file,
        u16_from_usize(central_directory.len())?,
        central_directory_size,
        central_directory_offset,
    )
}

fn write_local_entry(
    file: &mut File,
    entry: &ZipEntryInput<'_>,
) -> io::Result<CentralDirectoryEntry> {
    let name = entry.name.as_bytes();
    let name_len = u16_from_usize(name.len())?;
    let size = u32_from_usize(entry.content.len())?;
    let crc32 = crc32fast::hash(entry.content);
    let local_header_offset = u32_from_u64(file.stream_position()?)?;

    write_u32(file, LOCAL_FILE_HEADER_SIGNATURE)?;
    write_u16(file, VERSION_NEEDED)?;
    write_u16(file, 0)?;
    write_u16(file, STORED_METHOD)?;
    write_u16(file, 0)?;
    write_u16(file, 0)?;
    write_u32(file, crc32)?;
    write_u32(file, size)?;
    write_u32(file, size)?;
    write_u16(file, name_len)?;
    write_u16(file, 0)?;
    file.write_all(name)?;
    file.write_all(entry.content)?;

    Ok(CentralDirectoryEntry {
        name: name.to_vec(),
        crc32,
        size,
        local_header_offset,
    })
}

fn write_central_directory_entry(file: &mut File, entry: &CentralDirectoryEntry) -> io::Result<()> {
    write_u32(file, CENTRAL_DIRECTORY_HEADER_SIGNATURE)?;
    write_u16(file, VERSION_NEEDED)?;
    write_u16(file, VERSION_NEEDED)?;
    write_u16(file, 0)?;
    write_u16(file, STORED_METHOD)?;
    write_u16(file, 0)?;
    write_u16(file, 0)?;
    write_u32(file, entry.crc32)?;
    write_u32(file, entry.size)?;
    write_u32(file, entry.size)?;
    write_u16(file, u16_from_usize(entry.name.len())?)?;
    write_u16(file, 0)?;
    write_u16(file, 0)?;
    write_u16(file, 0)?;
    write_u16(file, 0)?;
    write_u32(file, 0)?;
    write_u32(file, entry.local_header_offset)?;
    file.write_all(&entry.name)
}

fn write_end_of_central_directory(
    file: &mut File,
    entry_count: u16,
    central_directory_size: u32,
    central_directory_offset: u32,
) -> io::Result<()> {
    write_u32(file, END_OF_CENTRAL_DIRECTORY_SIGNATURE)?;
    write_u16(file, 0)?;
    write_u16(file, 0)?;
    write_u16(file, entry_count)?;
    write_u16(file, entry_count)?;
    write_u32(file, central_directory_size)?;
    write_u32(file, central_directory_offset)?;
    write_u16(file, 0)
}

fn write_u16(file: &mut File, value: u16) -> io::Result<()> {
    file.write_all(&value.to_le_bytes())
}

fn write_u32(file: &mut File, value: u32) -> io::Result<()> {
    file.write_all(&value.to_le_bytes())
}

fn u16_from_usize(value: usize) -> io::Result<u16> {
    u16::try_from(value).map_err(|_| invalid_zip_size())
}

fn u32_from_usize(value: usize) -> io::Result<u32> {
    u32::try_from(value).map_err(|_| invalid_zip_size())
}

fn u32_from_u64(value: u64) -> io::Result<u32> {
    u32::try_from(value).map_err(|_| invalid_zip_size())
}

fn invalid_zip_size() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "diagnostics bundle is too large",
    )
}
