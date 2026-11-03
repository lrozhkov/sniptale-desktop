from __future__ import annotations

import struct
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "release/windows"))
import verify_windows_dependencies as dependencies


def executable(name: bytes = b"KERNEL32.dll") -> bytearray:
    value = bytearray(1024)
    value[:2] = b"MZ"
    struct.pack_into("<I", value, 0x3C, 128)
    value[128:132] = b"PE\0\0"
    struct.pack_into("<HH", value, 132, 0x8664, 1)
    struct.pack_into("<H", value, 148, 240)
    struct.pack_into("<H", value, 152, 0x20B)
    struct.pack_into("<I", value, 260, 16)
    struct.pack_into("<II", value, 272, 0x1000, 40)
    struct.pack_into("<III", value, 404, 0x1000, 512, 512)
    struct.pack_into("<I", value, 524, 0x1080)
    value[640 : 640 + len(name)] = name
    return value


class WindowsDependencyTests(unittest.TestCase):
    def test_system_imports_pass_but_runtime_and_other_external_dlls_fail(self):
        for name, accepted in (
            (b"KERNEL32.dll", True),
            (b"api-ms-win-crt-heap-l1-1-0.dll", True),
            (b"VCRUNTIME140.dll", False),
            (b"MSVCP140.dll", False),
            (b"thirdparty.dll", False),
        ):
            with self.subTest(name=name), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "agent.exe"
                path.write_bytes(executable(name))
                if accepted:
                    self.assertEqual(dependencies.verify(path), {name.decode().lower()})
                else:
                    with self.assertRaises(ValueError):
                        dependencies.verify(path)

    def test_malformed_imports_are_bounded_and_rejected(self):
        for mutation in ("truncated", "rva", "name", "long-name", "unterminated", "headers"):
            value = executable()
            if mutation == "truncated":
                value = value[:700]
            elif mutation == "rva":
                struct.pack_into("<I", value, 524, 0xFFFFFFFF)
            elif mutation == "name":
                value[640:645] = b"../xx"
            elif mutation == "long-name":
                value[640:896] = b"x" * 256
            elif mutation == "unterminated":
                struct.pack_into("<I", value, 276, 20)
            else:
                struct.pack_into("<H", value, 134, 65535)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                dependencies.imports(bytes(value))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "oversized.exe"
            with path.open("wb") as stream:
                stream.truncate(dependencies.MAX_PE_BYTES + 1)
            with self.assertRaises(ValueError):
                dependencies.verify(path)


if __name__ == "__main__":
    unittest.main()
