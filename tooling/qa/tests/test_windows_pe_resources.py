"""PE identity belongs to filenames and declared metadata, not arbitrary code bytes."""

from __future__ import annotations

import importlib.util
import struct
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
SPEC = importlib.util.spec_from_file_location(
    "windows_pe_resources", ROOT / "tooling/release/windows/verify_windows_pe_resources.py"
)
assert SPEC is not None and SPEC.loader is not None
PE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PE)


def version_block(key: str, value: bytes = b"", children: bytes = b"", *, text: bool = False) -> bytes:
    block = bytearray(struct.pack("<HHH", 0, len(value) // 2 if text else len(value), int(text)))
    block.extend((key + "\0").encode("utf-16le"))
    block.extend(b"\0" * (-len(block) % 4))
    block.extend(value)
    block.extend(b"\0" * (-len(block) % 4))
    block.extend(children)
    struct.pack_into("<H", block, 0, len(block))
    return bytes(block)


def version_info(product: str) -> bytes:
    name = version_block("ProductName", (product + "\0").encode("utf-16le"), text=True)
    table = version_block("040904b0", children=name)
    strings = version_block("StringFileInfo", children=table)
    fixed = struct.pack("<13I", 0xFEEF04BD, 0x10000, *([0] * 11))
    return version_block("VS_VERSION_INFO", fixed, strings)


def resources() -> dict[int, Any]:
    return {
        14: {icon: {1033: b"icon"} for icon in (101, 102, 103)},
        16: {1: {1033: version_info("Sniptale Desktop")}},
        24: {1: {1033: b"<assembly><description>Sniptale Desktop</description></assembly>"}},
    }


def executable(tree: dict[int, Any], code: bytes = b"code") -> bytes:
    resource = bytearray()

    def allocate(size: int) -> int:
        resource.extend(b"\0" * (-len(resource) % 4))
        offset = len(resource)
        resource.extend(b"\0" * size)
        return offset

    def directory(entries: dict[int, Any]) -> int:
        offset = allocate(16 + 8 * len(entries))
        struct.pack_into("<H", resource, offset + 14, len(entries))
        for index, (name, value) in enumerate(sorted(entries.items())):
            if isinstance(value, dict):
                target = directory(value) | 0x80000000
            else:
                target = allocate(16)
                data = allocate(len(value))
                resource[data : data + len(value)] = value
                struct.pack_into("<IIII", resource, target, 0x2000 + data, len(value), 0, 0)
            struct.pack_into("<II", resource, offset + 16 + index * 8, name, target)
        return offset

    assert directory(tree) == 0
    payload = bytearray(0x400 + len(resource))
    payload[:2] = b"MZ"
    struct.pack_into("<I", payload, 0x3C, 0x80)
    payload[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HH", payload, 0x84, 0x8664, 2)
    struct.pack_into("<H", payload, 0x94, 240)
    struct.pack_into("<H", payload, 0x98, 0x20B)
    struct.pack_into("<H", payload, 0x98 + 68, 2)
    struct.pack_into("<II", payload, 0x98 + 128, 0x2000, len(resource))
    for offset, name, rva, raw, size in (
        (0x188, b".text", 0x1000, 0x200, 0x200),
        (0x1B0, b".rsrc", 0x2000, 0x400, len(resource)),
    ):
        payload[offset : offset + len(name)] = name
        struct.pack_into("<IIII", payload, offset + 8, size, rva, size, raw)
    payload[0x200 : 0x200 + len(code)] = code
    payload[0x400:] = resource
    return bytes(payload)


class WindowsPeResourcesTests(unittest.TestCase):
    def verify(self, payload: bytes, name: str = "sniptale-native-agent.exe") -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / name
            path.write_bytes(payload)
            PE.verify(path)

    def test_valid_resources_and_non_identity_data_remain_allowed(self):
        self.verify(executable(resources()))
        for root in PE.retired_product_roots():
            for encoding in ("ascii", "utf-16le"):
                with self.subTest(root=root, encoding=encoding):
                    self.verify(executable(resources(), ("code-" + root.upper()).encode(encoding)))
            tree = resources()
            tree[14][101][1033] = ("binary-icon-" + root.upper()).encode()
            self.verify(executable(tree))

    def test_retired_filename_is_rejected(self):
        for root in PE.retired_product_roots():
            with self.subTest(root=root), self.assertRaises(PE.PeResourceError):
                self.verify(executable(resources()), root.upper() + ".exe")

    def test_retired_version_and_manifest_in_every_name_and_language_are_rejected(self):
        for root in PE.retired_product_roots():
            for kind in (16, 24):
                for name, language in ((1, 1033), (2, 1049)):
                    with self.subTest(root=root, kind=kind, name=name, language=language):
                        tree = resources()
                        value = version_info(root.upper()) if kind == 16 else root.upper().encode()
                        tree[kind].setdefault(name, {})[language] = value
                        with self.assertRaises(PE.PeResourceError):
                            self.verify(executable(tree))

    def test_missing_icons_and_manifest_still_fail(self):
        for kind, name in ((14, 101), (14, 102), (14, 103), (24, 1)):
            tree = resources()
            del tree[kind][name]
            with self.subTest(kind=kind, name=name), self.assertRaises(PE.PeResourceError):
                self.verify(executable(tree))

    def test_empty_or_malformed_declared_metadata_cannot_be_ignored(self):
        for value in (b"invalid-directory", {}, {1: {}}, {1: {1033: b""}}):
            tree = resources()
            tree[16] = value
            with self.subTest(value=value), self.assertRaises(PE.PeResourceError):
                self.verify(executable(tree))

    def test_invalid_headers_and_resource_rvas_still_fail(self):
        for offset, value in ((0, b"NO"), (0x98 + 68, b"\3\0"), (0x98 + 128, b"\xff" * 4)):
            payload = bytearray(executable(resources()))
            payload[offset : offset + len(value)] = value
            with self.subTest(offset=offset), self.assertRaises(PE.PeResourceError):
                self.verify(bytes(payload))
