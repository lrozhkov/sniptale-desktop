#!/usr/bin/env python3
"""Fail closed when a built Windows agent lacks its required GUI resources."""

from __future__ import annotations

import struct
import sys
import re
from pathlib import Path


RT_GROUP_ICON = 14
RT_MANIFEST = 24
WINDOWS_GUI_SUBSYSTEM = 2


class PeResourceError(ValueError):
    """The supplied file is not the expected Windows GUI resource layout."""


def retired_product_roots() -> tuple[str, ...]:
    return (
        "scr" + "eenyx",
        "smart" + "capture",
        "ns" + "mp",
        "s" + "cx",
    )


def retired_identity_strings(path: Path, payload: bytes) -> tuple[str, ...]:
    candidates = [path.name]
    candidates.extend(match.decode("ascii") for match in re.findall(rb"[\x20-\x7e]{3,}", payload))
    candidates.extend(
        match.decode("utf-16le")
        for match in re.findall(rb"(?:[\x20-\x7e]\x00){3,}", payload)
    )
    matches: list[str] = []
    for candidate in candidates:
        folded = candidate.casefold()
        if any(root in folded for root in retired_product_roots()):
            matches.append(candidate[:160])
    return tuple(matches)


def read_u16(payload: bytes, offset: int) -> int:
    return struct.unpack_from("<H", payload, offset)[0]


def read_u32(payload: bytes, offset: int) -> int:
    return struct.unpack_from("<I", payload, offset)[0]


def checked_slice(payload: bytes, offset: int, size: int) -> bytes:
    if offset < 0 or size < 0 or offset + size > len(payload):
        raise PeResourceError("truncated PE structure")
    return payload[offset : offset + size]


def rva_to_file_offset(sections: list[tuple[int, int, int, int]], rva: int) -> int:
    for virtual_address, virtual_size, raw_offset, raw_size in sections:
        span = max(virtual_size, raw_size)
        if virtual_address <= rva < virtual_address + span:
            return raw_offset + (rva - virtual_address)
    raise PeResourceError("resource RVA is outside PE sections")


def directory_entry(payload: bytes, base: int, directory_offset: int, resource_id: int) -> int:
    offset = base + directory_offset
    checked_slice(payload, offset, 16)
    count = read_u16(payload, offset + 12) + read_u16(payload, offset + 14)
    for index in range(count):
        entry = offset + 16 + (index * 8)
        checked_slice(payload, entry, 8)
        name = read_u32(payload, entry)
        value = read_u32(payload, entry + 4)
        if name == resource_id:
            return value
    raise PeResourceError(f"missing resource id {resource_id}")


def resource_data_size(payload: bytes, base: int, type_id: int, name_id: int) -> int:
    type_entry = directory_entry(payload, base, 0, type_id)
    if not type_entry & 0x8000_0000:
        raise PeResourceError(f"resource type {type_id} is not a directory")
    name_entry = directory_entry(payload, base, type_entry & 0x7fff_ffff, name_id)
    if not name_entry & 0x8000_0000:
        raise PeResourceError(f"resource {type_id}/{name_id} has no language directory")
    language_offset = base + (name_entry & 0x7fff_ffff)
    checked_slice(payload, language_offset, 24)
    language_count = read_u16(payload, language_offset + 12) + read_u16(payload, language_offset + 14)
    if language_count == 0:
        raise PeResourceError(f"resource {type_id}/{name_id} has no language entries")
    language_entry = language_offset + 16
    data_entry = read_u32(payload, language_entry + 4)
    if data_entry & 0x8000_0000:
        raise PeResourceError(f"resource {type_id}/{name_id} has an unexpected nested directory")
    data_offset = base + data_entry
    checked_slice(payload, data_offset, 16)
    size = read_u32(payload, data_offset + 4)
    if size == 0:
        raise PeResourceError(f"resource {type_id}/{name_id} is empty")
    return size


def verify(path: Path) -> None:
    payload = path.read_bytes()
    if retired_identity_strings(path, payload):
        raise PeResourceError("built executable filename or metadata contains a retired product root")
    if checked_slice(payload, 0, 2) != b"MZ":
        raise PeResourceError("missing DOS signature")
    pe_offset = read_u32(payload, 0x3C)
    if checked_slice(payload, pe_offset, 4) != b"PE\0\0":
        raise PeResourceError("missing PE signature")
    coff_offset = pe_offset + 4
    section_count = read_u16(payload, coff_offset + 2)
    optional_size = read_u16(payload, coff_offset + 16)
    optional_offset = coff_offset + 20
    if read_u16(payload, optional_offset) != 0x20B:
        raise PeResourceError("expected a PE32+ executable")
    if read_u16(payload, optional_offset + 68) != WINDOWS_GUI_SUBSYSTEM:
        raise PeResourceError("expected Windows GUI subsystem")
    if optional_size < 136:
        raise PeResourceError("truncated PE32+ optional header")
    resource_rva = read_u32(payload, optional_offset + 112 + (2 * 8))
    resource_size = read_u32(payload, optional_offset + 112 + (2 * 8) + 4)
    if resource_rva == 0 or resource_size == 0:
        raise PeResourceError("missing resource directory")
    section_offset = optional_offset + optional_size
    sections: list[tuple[int, int, int, int]] = []
    for index in range(section_count):
        offset = section_offset + (index * 40)
        checked_slice(payload, offset, 40)
        sections.append(
            (
                read_u32(payload, offset + 12),
                read_u32(payload, offset + 8),
                read_u32(payload, offset + 20),
                read_u32(payload, offset + 16),
            )
        )
    resource_base = rva_to_file_offset(sections, resource_rva)
    checked_slice(payload, resource_base, resource_size)
    for icon_id in (101, 102, 103):
        resource_data_size(payload, resource_base, RT_GROUP_ICON, icon_id)
    resource_data_size(payload, resource_base, RT_MANIFEST, 1)


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: verify_windows_pe_resources.py <sniptale-native-agent.exe>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    try:
        verify(path)
    except (OSError, PeResourceError, struct.error) as error:
        print(f"FAIL Windows PE resource verification: {path}: {error}", file=sys.stderr)
        return 1
    print(f"OK Windows PE resource verification: {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
