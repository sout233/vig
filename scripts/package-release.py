"""Package the native CI build; fail if its required ASR libraries are absent."""

import hashlib
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import zipfile


def package(target: str) -> None:
    build = Path("target") / target / "release"
    windows = target.endswith("windows-msvc")
    macos = target.endswith("apple-darwin")
    tag = os.environ["RELEASE_TAG"]
    name = f"vig-{re.sub(r'[^A-Za-z0-9._-]', '-', tag)}-{target}"
    directory = Path("dist") / name
    directory.mkdir(parents=True, exist_ok=False)
    executable = directory / ("vig.exe" if windows else "vig")
    shutil.copy2(build / executable.name, executable)
    libraries = [
        path for path in build.iterdir()
        if path.is_file() and (
            path.name.endswith((".dll", ".dylib", ".so")) or ".so." in path.name
        )
    ]
    for required in ("sherpa-onnx-c-api", "onnxruntime"):
        if not any(required in path.name for path in libraries):
            raise RuntimeError(f"Required native library is missing: {required}")
    for library in libraries:
        shutil.copy2(library, directory / library.name)
    if not windows and not macos:
        # sherpa-rs may copy a symlink's contents under its unversioned name.
        # Preserve each ELF SONAME as well, so DT_NEEDED resolves after unpacking.
        for library in libraries:
            path = directory / library.name
            soname = subprocess.check_output(["patchelf", "--print-soname", str(path)], text=True).strip()
            if soname and not (directory / soname).exists():
                shutil.copy2(path, directory / soname)
        libraries = [path for path in directory.iterdir() if path != executable]
    files = [executable, *(directory / path.name for path in libraries)]
    if macos:
        library_names = {path.name for path in libraries}
        for path in files:
            if path != executable:
                subprocess.run(["install_name_tool", "-id", f"@loader_path/{path.name}", str(path)], check=True)
            linked = subprocess.check_output(["otool", "-L", str(path)], text=True)
            for line in linked.splitlines()[1:]:
                dependency = line.strip().split(" (", 1)[0]
                basename = Path(dependency).name
                if basename in library_names:
                    subprocess.run([
                        "install_name_tool", "-change", dependency,
                        f"@loader_path/{basename}", str(path),
                    ], check=True)
        for path in files:
            subprocess.run(["codesign", "--force", "--sign", "-", str(path)], check=True)
    elif not windows:
        for path in files:
            subprocess.run(["patchelf", "--set-rpath", "$ORIGIN", str(path)], check=True)
    if not windows:
        executable.chmod(0o755)
    shutil.copy2("assets/icons/LICENSE", directory / "BOXICONS-LICENSE.txt")
    (directory / "BUILD.txt").write_text(
        f"Vig {tag}\nTarget: {target}\n"
        "Install ffmpeg and ffprobe on PATH before running Vig.\n"
        "Linux: a graphical X11/Wayland session and Vulkan driver are required;\n"
        "install ALSA, Fontconfig, XKB, GTK3, WebKitGTK 4.1 and Noto CJK fonts.\n"
        "macOS: requires macOS 15 or later. This build is ad-hoc signed, not notarized.\n"
        "Keep the supplied ASR libraries beside the executable.\n"
        "Model weights are downloaded separately inside the application.\n",
        encoding="utf-8",
    )
    if windows:
        archive = Path("dist") / f"{name}.zip"
        with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
            for path in directory.iterdir():
                output.write(path, Path(name) / path.name)
    else:
        archive = Path("dist") / f"{name}.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            output.add(directory, arcname=name)
    with archive.open("rb") as stream:
        checksum = hashlib.file_digest(stream, "sha256").hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(
        f"{checksum}  {archive.name}\n", encoding="utf-8",
    )
    # Upload the archives only; the staging directory is already archived.
    for path in directory.iterdir():
        path.unlink()
    directory.rmdir()
    print(archive)


if __name__ == "__main__":
    package(sys.argv[1])
