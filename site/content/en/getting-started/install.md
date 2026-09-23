<!-- source-hash: 4e9c5afd97ac -->
# Install

Install Tasty for your computer and open your first terminal. Below you will find installation steps for each OS, along with how to update or uninstall.

Download the installer from the [GitHub releases page](https://github.com/zilhak/tasty/releases). Install scripts and package-manager distribution (apt repositories, Homebrew, winget, and so on) are not currently available.

## Choosing an install file

`{ver}` stands for the version (e.g. `0.10.3`).

| OS | Architecture | File | Notes |
|----|---------|------|------|
| Linux | x86_64 | `tasty_{ver}-1_amd64.deb` | Debian / Ubuntu / Mint |
| Linux | x86_64 | `tasty-{ver}-1.x86_64.rpm` | Fedora / RHEL / openSUSE |
| Linux | x86_64 | `Tasty-{ver}-x86_64.AppImage` | Distribution-independent single file |
| Linux | x86_64 | `tasty-{ver}-linux-x64.tar.gz` | Archive for manual installation |
| Linux | aarch64 | `_arm64.deb` / `.aarch64.rpm` / `-aarch64.AppImage` / `-linux-arm64.tar.gz` | ARM64 |
| macOS | Apple Silicon | `Tasty-{ver}-macos-arm64.dmg` | Drag-and-drop install. There is no build for Intel Macs |
| Windows | x86_64 | `tasty-{ver}-windows-x64.msi` | Setup wizard (recommended) |
| Windows | x86_64 | `tasty-{ver}-windows-x64.zip` | Portable build — just unzip and run |

You can verify a downloaded file against the `SHA256SUMS-*.txt` for its OS.

## Linux

```sh
# .deb (Debian / Ubuntu / Mint)
sudo apt install ./tasty_{ver}-1_amd64.deb

# .rpm (Fedora / RHEL / openSUSE)
sudo dnf install ./tasty-{ver}-1.x86_64.rpm

# .AppImage — run without installing
chmod +x Tasty-{ver}-x86_64.AppImage && ./Tasty-{ver}-x86_64.AppImage
# If FUSE is not available
./Tasty-{ver}-x86_64.AppImage --appimage-extract-and-run

# .tar.gz — extract and run directly
tar -xzf tasty-{ver}-linux-x64.tar.gz && ./tasty-linux-x64/tasty
```

- `.deb` / `.rpm` put the `tasty` command on PATH and add an icon to the app menu. The package pulls in the required libraries automatically.
- GPU acceleration (Vulkan) is used when `libvulkan1` / `vulkan-loader` is present. Without it, Tasty still installs and runs, using OpenGL, or software rendering if that fails too.
- `.AppImage` bundles all libraries. You register it in the app menu yourself (use `appimaged`, or put a `.desktop` file in `~/.local/share/applications/`).
- `.tar.gz` requires you to set up PATH and the menu entry yourself. If a required system library is missing, `tasty` tells you what is missing and exits.
- The licence notices (Tasty's MIT text, the licence texts of the bundled font and markdown rendering engines, and the third-party notice document) travel inside the artifacts. `.deb` keeps them in `/usr/share/doc/tasty/`, `.rpm` and `.AppImage` in `usr/share/licenses/tasty/`, and `.tar.gz` at the top of the extracted directory. They are also set up to be uploaded to the release page, though no release has been published that way yet.
- The build baseline is Ubuntu 24.04 (glibc 2.39), so on older distributions (Ubuntu 20.04, Debian 11, etc.) it may fail to start with a `GLIBC_2.39 not found` error. No separate build is provided for older distributions.

## macOS

1. Open the `.dmg` and drag `Tasty.app` into the `Applications` folder.
2. If an "unidentified developer" warning appears on first launch, allow it in **System Settings > Privacy & Security**.

To clear the warning directly from the terminal:

```sh
xattr -dr com.apple.quarantine /Applications/Tasty.app
```

Right after the first launch, macOS permission prompts (Downloads · Documents · Desktop folders, Screen Recording) appear one after another. Why they appear and how to answer them is in [Troubleshooting](../help/troubleshooting.md#macos-permission-prompts).

## Windows

- **`.msi` (recommended)** — Double-click and follow the setup wizard. A Start menu shortcut and an "Apps & features" uninstall entry are registered.
- **`.zip`** — Extract to any folder and run `tasty.exe`.

```powershell
Expand-Archive tasty-{ver}-windows-x64.zip -DestinationPath tasty
.\tasty\tasty.exe
```

On first launch a "Windows protected your PC" warning appears (the binary is not code-signed). Click **More info → Run anyway**.

On Windows, Tasty uses **Git Bash** as its shell. If Git for Windows is not installed, the settings window shows a "Git Bash not found" notice, so either install it first or set the bash path yourself under **Settings** > **Terminal** > **Shell**.

## First launch

- When the window opens, one Workspace and one terminal are ready. The screen layout is explained in [A first look](first-look.md).
- The UI language defaults to English. To switch to Korean, pick Korean under **Settings** > **General** > **Language**, save, and restart Tasty. You can also write it directly into `~/.tasty/config.toml`:

```toml
[general]
language = "ko"
```

Check from the terminal that the install worked. The second command only responds while Tasty is running.

```sh
tasty --version      # e.g. tasty 0.10.3
tasty list info      # version · Workspace count of the running Tasty
```

Shells opened inside Tasty get the `tasty` command on PATH automatically. On macOS, to use it from other terminal apps or scripts as well, add `/Applications/Tasty.app/Contents/MacOS/tasty` to PATH or create a symbolic link. The Windows `.zip` build likewise needs the extracted folder added to PATH.

## GPU requirements

Tasty draws its screen with the GPU (Vulkan / DirectX 12 / Metal). If there is no GPU it tries once more with a software renderer, and if that fails too it prints a "GPU adapter not found" message and exits. Installing or updating the GPU driver resolves this in most cases. All distributed install files are GUI builds, so they do not run on servers without a GPU.

## Headless build

A **headless build**, which runs Tasty on a server without a window, is not distributed as an install file. Running a distributed Tasty with `--headless` does not start it without a window; it starts the GUI as usual.

If you need a headless build, build it yourself from source on a machine with Rust installed.

```sh
git clone https://github.com/zilhak/tasty.git
cd tasty
cargo build --release --no-default-features
./target/release/tasty
```

The `tasty` built this way starts without a window even with no options, and you create and drive its terminals only through the `tasty` CLI. From another computer you can bring its workspaces in with [remote attach](../remote/attach.md).

## Updating

There is no auto-update and no new-version notification. Download the new version from the releases page and install it again the same way.

- Linux `.deb` / `.rpm`: run the same command with the new file and it installs over the old one.
- macOS: overwrite `Tasty.app` in `Applications` with the new one.
- Windows `.msi`: running the new `.msi` upgrades it. User data (`~/.tasty/`) is left as is.

Settings · sessions · themes all live in `~/.tasty/`, so they survive updates.

## Uninstalling

| OS | Remove the program | User data |
|----|--------------|--------------|
| Linux `.deb` | `sudo apt remove tasty` | `~/.tasty/` remains. Delete it yourself |
| Linux `.rpm` | `sudo dnf remove tasty` | Same as above |
| Linux AppImage / tar.gz | Delete the file · folder | Same as above |
| macOS | Move `Tasty.app` to the Trash | Same as above |
| Windows `.msi` | Uninstall Tasty from **Settings > Apps** | **`~/.tasty/` is deleted along with it** — back it up before uninstalling if you want to keep settings · sessions · themes |
| Windows `.zip` | Delete the extracted folder | `~/.tasty/` remains. Delete it yourself |

On Windows, `~` is `%USERPROFILE%` (usually `C:\Users\<name>`).

## Install locations

| OS | Executable | User data |
|----|----------|--------------|
| Linux | `/usr/bin/tasty` (`.deb` / `.rpm`) or wherever you extracted it | `~/.tasty/` |
| macOS | `/Applications/Tasty.app/Contents/MacOS/tasty` | `~/.tasty/` |
| Windows | `C:\Program Files\tasty\bin\tasty.exe` (`.msi`) | `%USERPROFILE%\.tasty\` |

`~/.tasty/` holds the settings (`config.toml`), the IPC port of the running Tasty (`tasty.port`), layouts, themes, and logs. The main files are covered in [Settings](../customize/settings.md) and [Troubleshooting](../help/troubleshooting.md).
