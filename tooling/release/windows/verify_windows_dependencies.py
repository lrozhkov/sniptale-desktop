"""Bounded PE import admission for the standalone Windows alpha executable."""
from __future__ import annotations

import re
import struct
import sys
from pathlib import Path

MAX_PE_BYTES = 64 * 1024 * 1024
SYSTEM_DLLS = {
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "combase.dll", "comctl32.dll",
    "crypt32.dll", "d2d1.dll", "d3d11.dll", "dbghelp.dll", "dwmapi.dll", "dwrite.dll",
    "gdi32.dll", "kernel32.dll", "mfplat.dll", "mfreadwrite.dll", "msvcrt.dll", "ntdll.dll",
    "ole32.dll", "oleaut32.dll", "propsys.dll", "rpcrt4.dll", "secur32.dll", "shell32.dll",
    "ucrtbase.dll", "user32.dll", "userenv.dll", "uxtheme.dll", "version.dll", "winmm.dll",
    "wintrust.dll", "ws2_32.dll",
}


def imports(payload: bytes) -> set[str]:
    def take(offset: int, size: int) -> memoryview:
        if offset < 0 or size < 0 or offset + size > len(payload):
            raise ValueError("truncated PE import structure")
        return memoryview(payload)[offset:offset + size]

    def u16(offset: int) -> int:
        return struct.unpack("<H", take(offset, 2))[0]

    def u32(offset: int) -> int:
        return struct.unpack("<I", take(offset, 4))[0]

    if len(payload) > MAX_PE_BYTES or take(0, 2) != b"MZ":
        raise ValueError("invalid or oversized PE executable")
    pe = u32(0x3c)
    if take(pe, 4) != b"PE\0\0" or u16(pe + 4) != 0x8664:
        raise ValueError("expected Windows x64 PE executable")
    count, optional_size = u16(pe + 6), u16(pe + 20)
    optional = pe + 24
    if not 1 <= count <= 96 or optional_size < 128 or u16(optional) != 0x20b or u32(optional + 108) < 2:
        raise ValueError("invalid PE32+ import headers")
    take(optional, optional_size)
    sections = []
    for index in range(count):
        section = optional + optional_size + index * 40
        take(section, 40)
        address, size, raw = u32(section + 12), u32(section + 16), u32(section + 20)
        take(raw, size)
        sections.append((address, size, raw))

    def rva(address: int, size: int) -> int:
        for start, length, raw in sections:
            if start <= address and address + size <= start + length:
                return raw + address - start
        raise ValueError("PE import RVA is outside file-backed sections")

    directory, directory_size = u32(optional + 120), u32(optional + 124)
    if not directory or directory_size < 20:
        raise ValueError("missing PE import directory")
    names = set()
    for index in range(257):
        if index * 20 + 20 > directory_size:
            raise ValueError("unterminated PE import directory")
        entry = rva(directory + index * 20, 20)
        if take(entry, 20) == bytes(20):
            if not names:
                raise ValueError("empty PE import directory")
            return names
        if index == 256:
            raise ValueError("too many PE imports")
        name_rva = u32(entry + 12)
        name = bytearray()
        for offset in range(256):
            byte = take(rva(name_rva + offset, 1), 1)[0]
            if byte == 0:
                break
            name.append(byte)
        else:
            raise ValueError("oversized PE import name")
        if not re.fullmatch(rb"[A-Za-z0-9_.-]+\.dll", bytes(name), re.IGNORECASE):
            raise ValueError("invalid PE import name")
        names.add(name.decode("ascii").lower())
    raise ValueError("unterminated PE imports")


def verify(path: Path) -> set[str]:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_PE_BYTES:
        raise ValueError("missing, redirected or oversized Windows executable")
    with path.open("rb") as stream:
        names = imports(stream.read(MAX_PE_BYTES + 1))
    if any(name not in SYSTEM_DLLS and not name.startswith(("api-ms-win-", "ext-ms-win-")) for name in names):
        raise ValueError("standalone alpha imports a non-system DLL")
    return names


if __name__ == "__main__":
    verify(Path(sys.argv[1]))
    print("Windows system dependency closure verified")
