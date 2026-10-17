"""Read regular and delay-load PE imports without running the executable."""

from pathlib import Path
import struct


SYSTEM_DLLS = set("""
advapi32.dll bcrypt.dll bcryptprimitives.dll cabinet.dll cfgmgr32.dll combase.dll
comctl32.dll comdlg32.dll crypt32.dll cryptbase.dll cryptnet.dll cryptui.dll d2d1.dll
d3d11.dll d3d12.dll d3dcompiler_47.dll dbghelp.dll dcomp.dll dhcpcsvc.dll dnsapi.dll
dwmapi.dll dwrite.dll dxgi.dll gdi32.dll gdiplus.dll hid.dll icu.dll icuin.dll icuuc.dll
imagehlp.dll imm32.dll iphlpapi.dll kernel32.dll kernelbase.dll mf.dll mfplat.dll
mfreadwrite.dll mpr.dll msvcrt.dll mswsock.dll ncrypt.dll netapi32.dll normaliz.dll
ntdll.dll ole32.dll oleacc.dll oleaut32.dll opengl32.dll powrprof.dll propsys.dll
psapi.dll rpcrt4.dll secur32.dll setupapi.dll shell32.dll shcore.dll shlwapi.dll
sspicli.dll ucrtbase.dll uiautomationcore.dll user32.dll userenv.dll usp10.dll
uxtheme.dll version.dll windowscodecs.dll winhttp.dll wininet.dll winmm.dll
winscard.dll winspool.drv wintrust.dll winusb.dll wlanapi.dll ws2_32.dll wtsapi32.dll
""".split())


def imported_dlls(path: Path) -> set[str]:
    data = path.read_bytes()
    if data[:2] != b"MZ":
        raise ValueError(f"Not a PE executable: {path}")
    pe = struct.unpack_from("<I", data, 60)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError(f"Invalid PE signature: {path}")
    section_count = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    if magic not in (0x10B, 0x20B):
        raise ValueError("Unsupported PE optional header")
    pe64 = magic == 0x20B
    directories = optional + (112 if pe64 else 96)
    image_base = struct.unpack_from("<Q" if pe64 else "<I", data, optional + (24 if pe64 else 28))[0]
    sections = [
        struct.unpack_from("<IIII", data, optional + optional_size + index * 40 + 8)
        for index in range(section_count)
    ]

    def offset(rva: int) -> int:
        for virtual_size, address, raw_size, raw_offset in sections:
            if address <= rva < address + max(virtual_size, raw_size):
                return raw_offset + rva - address
        raise ValueError(f"PE RVA not mapped to a section: {rva:x}")

    imports = set()
    for index, stride, name_offset in ((1, 20, 12), (13, 32, 4)):
        rva = struct.unpack_from("<I", data, directories + index * 8)[0]
        if not rva:
            continue
        descriptor = offset(rva)
        while any(data[descriptor:descriptor + stride]):
            name = struct.unpack_from("<I", data, descriptor + name_offset)[0]
            if index == 13 and not struct.unpack_from("<I", data, descriptor)[0] & 1:
                name -= image_base
            start = offset(name)
            imports.add(data[start:data.index(b"\0", start)].decode("ascii").lower())
            descriptor += stride
    return imports


def verify_standalone(path: Path) -> None:
    unexpected = sorted(
        name for name in imported_dlls(path)
        if name not in SYSTEM_DLLS and not name.startswith(("api-ms-win-", "ext-ms-"))
    )
    if unexpected:
        raise ValueError(f"Single EXE requires non-system DLLs: {', '.join(unexpected)}")
    print(f"Single EXE imports verified: {path.name} uses Windows system DLLs only")
