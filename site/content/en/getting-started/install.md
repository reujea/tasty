<!-- source-hash: 1f752da03c3a -->
# Installation

Choose the installation file for your OS and architecture. macOS support is for Apple Silicon. Installation files are available on the GitHub releases page.

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
| Linux | x86_64 | `tasty-headless-{ver}-linux-x64.tar.gz` | [Headless build](#headless-build) (runs on a server without a window) |
| Linux | aarch64 | `tasty-headless-{ver}-linux-arm64.tar.gz` | [Headless build](#headless-build) |
| macOS | Apple Silicon | `Tasty-{ver}-macos-arm64.dmg` | Drag-and-drop install. There is no build for Intel Macs |
| macOS | Apple Silicon | `tasty-headless-{ver}-macos-arm64.tar.gz` | [Headless build](#headless-build) |
| Windows | x86_64 | `tasty-{ver}-windows-x64.msi` | Setup wizard (recommended) |
| Windows | x86_64 | `tasty-{ver}-windows-x64.zip` | Portable build — just unzip and run |
| Windows | x86_64 | `tasty-headless-{ver}-windows-x64.zip` | [Headless build](#headless-build) |
| Windows | ARM64 | `tasty-{ver}-windows-arm64.msi` | Setup wizard. Not in releases up to 0.12.2. See [Windows](#windows) |
| Windows | ARM64 | `tasty-{ver}-windows-arm64.zip` | Portable build — just unzip and run |
| Windows | ARM64 | `tasty-headless-{ver}-windows-arm64.zip` | [Headless build](#headless-build) |

You can verify a downloaded file against the `SHA256SUMS-*.txt` for its OS.

The download button on the site's front page links straight to a file only when one matches both the OS and the architecture your browser reports. When the architecture is unknown it opens the releases page instead, so pick the file for your computer's architecture from the table above.

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

- `.deb` / `.rpm` put the `tasty` command on PATH and add an icon to the app menu. The package manager installs the required libraries.
- GPU acceleration (Vulkan) is used when `libvulkan1` / `vulkan-loader` is present. Without it, Tasty still installs and runs, using OpenGL, or software rendering if that fails too.
- `zenity` is a recommended package. Tasty uses it to show a notice when a new launch cannot be handed to the Tasty that is already running. Without it, a desktop notification is shown instead.
- `.AppImage` bundles libraries used to run Tasty. You register it in the app menu yourself (use `appimaged`, or put a `.desktop` file in `~/.local/share/applications/`).
- `.tar.gz` requires you to set up PATH and the menu entry yourself. If a required system library is missing, `tasty` tells you what is missing and exits.
- The licence notices (Tasty's MIT text, the licence texts of the bundled font and markdown rendering engines, and the third-party notice document) travel inside the artifacts. `.deb` keeps them in `/usr/share/doc/tasty/`, `.rpm` and `.AppImage` in `usr/share/licenses/tasty/`, and `.tar.gz` at the top of the extracted directory.
- The build baseline is Ubuntu 24.04 (glibc 2.39), so on older distributions (Ubuntu 20.04, Debian 11, etc.) it may fail to start with a `GLIBC_2.39 not found` error. No separate build is provided for older distributions.
- The HTML and Markdown viewers need WebKitGTK 2.40 or later. The `libwebkit2gtk-4.1` of the Ubuntu 24.04 build baseline meets this.

## macOS

1. Open the `.dmg` and drag `Tasty.app` into the `Applications` folder.
2. If an "unidentified developer" warning appears on first launch, allow it in **System Settings > Privacy & Security**.

To clear the warning directly from the terminal:

```sh
xattr -dr com.apple.quarantine /Applications/Tasty.app
```

After the first launch, macOS may show permission prompts for the Downloads, Documents, and Desktop folders, followed by Screen Recording. Which prompts appear depends on prior permission choices and macOS policy. Why they appear and how to answer them is in [Troubleshooting](../help/troubleshooting.md#macos-permission-prompts).

## Windows

On a Windows ARM device, download the files with `windows-arm64` in the name. ARM64 files ship in releases newer than 0.12.2, and installing and running them on a Windows ARM device has not been checked yet. Running the x64 files of releases up to 0.12.2, which have no ARM64 files, on a Windows ARM device has not been checked either.

Windows ARM is supported. The developer’s access to hardware for direct testing is limited. Platform support and the scope verified for an individual release are separate.

- **`.msi` (recommended)** — Double-click and follow the setup wizard. A Start menu shortcut and an "Apps & features" uninstall entry are registered.
- **`.zip`** — Extract to any folder and run `tasty.exe`.

```powershell
Expand-Archive tasty-{ver}-windows-x64.zip -DestinationPath tasty
.\tasty\tasty.exe
```

On first launch a "Windows protected your PC" warning appears (the binary is not code-signed). Click **More info → Run anyway**.

On Windows, Tasty uses **Git Bash** as its shell. If you start Tasty for the first time without Git for Windows, a **Choose a shell** screen appears instead of a terminal. Enter the path to bash.exe and press **Use this shell** to start with that shell, or press **Quit** to close Tasty. **Use this shell** is available only when the path is a bash or zsh executable. Once Tasty is running, if the configured shell path is not valid, the **Terminal** tab of the settings window shows a "Git Bash not found" notice. In that case install Git for Windows, or set the bash path yourself under **Settings** > **Terminal** > **Shell**.

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

Tasty draws its screen with the GPU (Vulkan / DirectX 12 / Metal). If there is no GPU it tries once more with a software renderer, and if that fails too it exits without opening a window. No error is shown on screen; "No GPU adapter found" is written to the log (`~/.tasty/debug.log`, and the terminal output when launched from a terminal). Installing or updating the GPU driver resolves this in most cases. Every install file without `headless` in its name is a GUI build. To run on a server without a graphical desktop, use the headless build below.

## Headless build

A **headless build**, which runs Tasty on a server without a window, is published on the releases page as an archive for each OS (named `tasty-headless-…`). You just extract it; it is not distributed as an install file such as `.deb`, `.rpm`, `.AppImage`, `.dmg`, or `.msi`. The executable is named `tasty` in both the GUI and headless builds, so extract it into a different folder from the GUI build. The bundled plugins are included in the archive's `plugins/` folder.

```sh
# Linux (for aarch64, replace x64 with arm64)
tar -xzf tasty-headless-{ver}-linux-x64.tar.gz && ./tasty-headless-linux-x64/tasty

# macOS (Apple Silicon)
tar -xzf tasty-headless-{ver}-macos-arm64.tar.gz && ./tasty-headless-macos-arm64/tasty
```

```powershell
# Windows (for ARM64, replace x64 with arm64)
Expand-Archive tasty-headless-{ver}-windows-x64.zip -DestinationPath tasty-headless
.\tasty-headless\tasty.exe
```

On macOS, a file downloaded with a browser may show an "unidentified developer" warning when you run it. In that case, clear the quarantine flag with `xattr -dr com.apple.quarantine tasty-headless-macos-arm64`.

The headless build's `tasty` starts without a window even with no options, and you create and drive its terminals only through the `tasty` CLI. From another computer you can bring its workspaces in with [remote attach](../remote/attach.md). Running a GUI build with `--headless` does not start it without a window; it starts the GUI as usual.

Where no archive is published (for example, an Intel Mac), build it yourself from source on a machine with Rust installed.

```sh
git clone https://github.com/zilhak/tasty.git
cd tasty
cargo build --release --no-default-features
./target/release/tasty
```

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
| Headless archive (any OS) | Delete the extracted folder | `~/.tasty/` remains. Delete it yourself |

On Windows, `~` is `%USERPROFILE%` (usually `C:\Users\<name>`).

## Install locations

| OS | Executable | User data |
|----|----------|--------------|
| Linux | `/usr/bin/tasty` (`.deb` / `.rpm`) or wherever you extracted it | `~/.tasty/` |
| macOS | `/Applications/Tasty.app/Contents/MacOS/tasty` | `~/.tasty/` |
| Windows | `C:\Program Files\tasty\bin\tasty.exe` (`.msi`) | `%USERPROFILE%\.tasty\` |

`~/.tasty/` holds the settings (`config.toml`), the IPC port of the running Tasty (`tasty.port`), layouts, themes, and logs. The main files are covered in [Settings](../customize/settings.md) and [Troubleshooting](../help/troubleshooting.md).
