"""Package the MATLAB source and the native MEX built for this runner."""

import argparse
import platform
import re
import shutil
import sys
import tempfile
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


PLATFORMS = {
    "windows-x64": ("win32", "x86_64", "omnibci_matlab_native.dll", "mexw64"),
    "linux-x64": ("linux", "x86_64", "libomnibci_matlab_native.so", "mexa64"),
    "macos-x64": ("darwin", "x86_64", "libomnibci_matlab_native.dylib", "mexmaci64"),
    "macos-arm64": ("darwin", "arm64", "libomnibci_matlab_native.dylib", "mexmaca64"),
}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--platform", required=True, choices=PLATFORMS)
    args = parser.parse_args()
    if not re.fullmatch(r"\d+\.\d+\.\d+", args.version):
        parser.error("version must be X.Y.Z")

    system, arch, library_name, mex_extension = PLATFORMS[args.platform]
    actual_arch = platform.machine().lower()
    if actual_arch in ("amd64", "x64"):
        actual_arch = "x86_64"
    elif actual_arch == "aarch64":
        actual_arch = "arm64"
    if (sys.platform, actual_arch) != (system, arch):
        parser.error(f"{args.platform} does not match runner {sys.platform}/{actual_arch}")

    root = Path(__file__).resolve().parent.parent
    library = root / "target" / "release" / library_name
    if not library.is_file():
        parser.error(f"build the native MEX first: {library}")
    name = f"omnibci-matlab-v{args.version}-{args.platform}"
    dist = root / "dist"
    dist.mkdir(exist_ok=True)
    archive = dist / f"{name}.zip"

    with tempfile.TemporaryDirectory(prefix="omnibci-package-", dir=dist) as temp:
        stage = Path(temp) / name
        for source in (root / "matlab").rglob("*.m"):
            destination = stage / "matlab" / source.relative_to(root / "matlab")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        destination = stage / "matlab" / "+omnibci" / "private" / f"omnibci_mex.{mex_extension}"
        shutil.copy2(library, destination)
        for source_name, destination_name in (
            ("README.md", "README.md"),
            ("LICENSE", "LICENSE"),
            ("sdk/LICENSE", "SDK-LICENSE"),
        ):
            shutil.copy2(root / source_name, stage / destination_name)
        shutil.copytree(root / "examples", stage / "examples")
        with ZipFile(archive, "w", compression=ZIP_DEFLATED) as output:
            for source in sorted(stage.rglob("*")):
                if source.is_file():
                    output.write(source, source.relative_to(Path(temp)))
    print(f"Created {archive}")


if __name__ == "__main__":
    main()
