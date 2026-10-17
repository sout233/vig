"""Regression tests for edition packaging and Windows DLL dependency checks."""

import os
from pathlib import Path
import runpy
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from pe_imports import imported_dlls, verify_standalone

package = runpy.run_path(str(Path(__file__).with_name("package-release.py")))["package"]


def pe_fixture(normal: str, delayed: str) -> bytes:
    data = bytearray(0x600)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 60, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x86, 1)
    struct.pack_into("<H", data, 0x94, 240)
    struct.pack_into("<H", data, 0x98, 0x20B)
    struct.pack_into("<Q", data, 0x98 + 24, 0x140000000)
    struct.pack_into("<I", data, 0x98 + 108, 16)
    struct.pack_into("<II", data, 0x98 + 112 + 8, 0x1000, 40)
    struct.pack_into("<II", data, 0x98 + 112 + 13 * 8, 0x1200, 64)
    struct.pack_into("<IIII", data, 0x98 + 240 + 8, 0x400, 0x1000, 0x400, 0x200)
    struct.pack_into("<IIIII", data, 0x200, 0, 0, 0, 0x1100, 1)
    struct.pack_into("<II", data, 0x400, 1, 0x1300)
    for offset, name in ((0x300, normal), (0x500, delayed)):
        encoded = name.encode("ascii") + b"\0"
        data[offset:offset + len(encoded)] = encoded
    return bytes(data)


class ReleaseTests(unittest.TestCase):
    def test_regular_and_delay_imports_are_both_audited(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "fixture.exe"
            path.write_bytes(pe_fixture("KERNEL32.dll", "VCRUNTIME140.dll"))
            self.assertEqual(imported_dlls(path), {"kernel32.dll", "vcruntime140.dll"})
            with self.assertRaisesRegex(ValueError, "vcruntime140.dll"):
                verify_standalone(path)
            path.write_bytes(pe_fixture("sherpa-onnx-c-api.dll", "USER32.dll"))
            with self.assertRaisesRegex(ValueError, "sherpa-onnx-c-api.dll"):
                verify_standalone(path)

    def test_lite_is_one_exe_and_full_keeps_only_used_asr_libraries(self):
        target = "x86_64-pc-windows-msvc"
        with tempfile.TemporaryDirectory() as temporary:
            previous = Path.cwd()
            try:
                os.chdir(temporary)
                build = Path("target") / target / "release"
                build.mkdir(parents=True)
                (build / "vig.exe").write_bytes(pe_fixture("KERNEL32.dll", "USER32.dll"))
                for name in ("sherpa-onnx-c-api.dll", "onnxruntime.dll", "onnxruntime_providers_shared.dll", "cargs.dll", "sherpa-onnx-cxx-api.dll"):
                    (build / name).write_bytes(b"native library packaging fixture")
                license_file = Path("assets/icons/LICENSE")
                license_file.parent.mkdir(parents=True)
                license_file.write_text("license fixture", encoding="utf-8")
                with patch.dict(os.environ, RELEASE_TAG="test"):
                    package(target, "lite")
                    lite = Path("dist") / f"vig-test-lite-{target}.exe"
                    self.assertEqual({p.name for p in Path("dist").iterdir()}, {lite.name, lite.name + ".sha256"})
                    package(target, "full")
                with zipfile.ZipFile(Path("dist") / f"vig-test-full-{target}.zip") as archive:
                    names = {Path(name).name for name in archive.namelist()}
                self.assertTrue({"vig.exe", "sherpa-onnx-c-api.dll", "onnxruntime.dll", "onnxruntime_providers_shared.dll"} <= names)
                self.assertFalse({"cargs.dll", "sherpa-onnx-cxx-api.dll"} & names)
            finally:
                os.chdir(previous)


if __name__ == "__main__":
    unittest.main()
