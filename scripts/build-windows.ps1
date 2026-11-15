# Build Windows distributables for Tasty.
#
# Usage:
#   .\scripts\build-windows.ps1              # dist build (full LTO, 배포용), x64
#   .\scripts\build-windows.ps1 -Arch arm64  # aarch64-pc-windows-msvc 산출물 (x64 호스트에서는 교차 빌드)
#   .\scripts\build-windows.ps1 -Release     # release build (thin LTO, 빠른 빌드)
#   .\scripts\build-windows.ps1 -Debug       # debug build
#   .\scripts\build-windows.ps1 -SkipMsi     # ZIP만 만들고 MSI는 건너뜀
#
# Output ({arch} = x64 | arm64):
#   dist\tasty-{version}-windows-{arch}.zip   (portable)
#   dist\tasty-{version}-windows-{arch}.msi   (installer; cargo-wix + WiX 3.x auto-installed if missing)
#   dist\tasty-headless-{version}-windows-{arch}.zip   (gui feature 없는 헤드리스 빌드, portable)
#   dist\SHA256SUMS-windows.txt (x64) / dist\SHA256SUMS-windows-arm64.txt (arm64)
#
# Requires: Git for Windows (Git Bash, curl, tar). 번들 플러그인은 official-plugins.lock.toml의
# 산출물을 받는다(빌드하지 않는다). 받기·서명 스크립트는 Git Bash로 실행한다.

param(
    [ValidateSet("x64", "arm64")]
    [string]$Arch = "x64",
    [switch]$Release,
    [switch]$Debug,
    [switch]$SkipMsi
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    Write-Error "This script must be run on Windows."
    exit 1
}

Push-Location (Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path))
try {

# $Profile은 PowerShell 자동 변수이므로 빌드 프로필에는 $BuildProfile을 사용한다.
$BuildProfile = "dist"
$CargoFlags = @("--profile", "dist")
if ($Debug) {
    $BuildProfile = "debug"
    $CargoFlags = @()
} elseif ($Release) {
    $BuildProfile = "release"
    $CargoFlags = @("--release")
}

# 호스트 아키텍처로 빌드하면 러너에 따라 이름과 실제 아키텍처가 어긋날 수 있으므로 타깃을 항상 지정한다.
# 타깃을 지정하면 산출물은 target\<triple>\<profile>\ 에 생긴다.
$Targets = @{
    x64   = @{ Triple = "x86_64-pc-windows-msvc";  Machine = 0x8664 }
    arm64 = @{ Triple = "aarch64-pc-windows-msvc"; Machine = 0xAA64 }
}
$Triple = $Targets[$Arch].Triple
$ExpectedMachine = $Targets[$Arch].Machine
$CargoFlags += @("--target", $Triple)

# 다른 아키텍처의 실행 파일은 이 호스트에서 실행할 수 없으므로 실행 검사는 같은 아키텍처일 때만 한다.
$HostArch = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    "X64" { "x64" }
    "Arm64" { "arm64" }
    default { $_ }
}
$CanRunTarget = ($HostArch -eq $Arch)
Write-Host "==> Target $Triple ($Arch) on $HostArch host"

$Installed = @(rustup target list --installed)
if ($LASTEXITCODE -ne 0) {
    Write-Error "rustup target list failed with exit code $LASTEXITCODE"
    exit 1
}
if ($Installed -notcontains $Triple) {
    Write-Host "==> Installing Rust target $Triple..."
    rustup target add $Triple
    if ($LASTEXITCODE -ne 0) {
        Write-Error "rustup target add $Triple failed with exit code $LASTEXITCODE"
        exit 1
    }
}

# C 의존성(libsqlite3-sys 등)은 대상 아키텍처의 MSVC 도구로 컴파일한다.
if ($Arch -eq "arm64") {
    $Vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $Vswhere) {
        $Arm64Tools = & $Vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath
        if (-not $Arm64Tools) {
            Write-Error "MSVC ARM64 build tools not found. Add the Visual Studio component Microsoft.VisualStudio.Component.VC.Tools.ARM64 (MSVC ARM64 build tools)."
            exit 1
        }
        # aws-lc-sys assembles its AArch64 .S files with clang-cl. cl.exe skips them and the link then fails.
        $Arm64Clang = & $Vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Llvm.Clang -property installationPath
        if (-not $Arm64Clang) {
            Write-Error "clang-cl not found. Add the Visual Studio component Microsoft.VisualStudio.Component.VC.Llvm.Clang (C++ Clang Compiler for Windows)."
            exit 1
        }
    } else {
        Write-Warning "vswhere.exe not found; cannot confirm the MSVC ARM64 build tools before building."
    }
}

$CargoContent = Get-Content "Cargo.toml" -Raw
if ($CargoContent -match '(?m)^version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    Write-Error "Failed to extract version from Cargo.toml"
    exit 1
}

$DistDir = "dist"
$ArchiveName = "tasty-${Version}-windows-${Arch}.zip"
$MsiName = "tasty-${Version}-windows-${Arch}.msi"
$HeadlessArchiveName = "tasty-headless-${Version}-windows-${Arch}.zip"
# x64 체크섬 파일은 기존 이름을 유지한다. 아키텍처마다 파일이 달라 서로 덮지 않는다.
$ShaSumsName = if ($Arch -eq "x64") { "SHA256SUMS-windows.txt" } else { "SHA256SUMS-windows-${Arch}.txt" }
# 두 아키텍처를 같은 dist에서 만들어도 staging 경로가 겹치지 않게 한다.
$StageDir = Join-Path $DistDir "tasty-windows-${Arch}"
$BuildDir = Join-Path (Join-Path "target" $Triple) $BuildProfile

# WSL bash가 선택되는 일을 줄이기 위해 Git Bash를 우선 찾는다.
function Resolve-Bash {
    $candidates = @()
    $gitCmd = Get-Command git -ErrorAction SilentlyContinue
    if ($gitCmd) {
        $gitRoot = Split-Path (Split-Path $gitCmd.Source -Parent) -Parent
        $candidates += (Join-Path $gitRoot 'bin\bash.exe')
        $candidates += (Join-Path $gitRoot 'usr\bin\bash.exe')
    }
    $candidates += 'C:\Program Files\Git\bin\bash.exe'
    $candidates += 'C:\Program Files\Git\usr\bin\bash.exe'
    if (${env:ProgramFiles(x86)}) {
        $candidates += (Join-Path ${env:ProgramFiles(x86)} 'Git\bin\bash.exe')
    }
    foreach ($c in $candidates) {
        if ($c -and (Test-Path $c)) { return $c }
    }
    $b = Get-Command bash -ErrorAction SilentlyContinue
    if ($b) { return $b.Source }
    return $null
}

# 빌드 전에 서명 키를 준비해 대응 공개키가 바이너리에 포함되게 한다.
if ($BuildProfile -ne "debug") {
    $SignKeyPath = $env:SIGN_KEY_PATH
    if (-not $SignKeyPath) {
        $ReleaseKey = Join-Path $env:USERPROFILE ".tasty-keys\release.pem"
        $DevKey = Join-Path $env:USERPROFILE ".tasty-keys\dev.pem"
        if (Test-Path $ReleaseKey) {
            $SignKeyPath = $ReleaseKey
        } else {
            # 개발 키가 없으면 만들고 기존 키가 있으면 공개키를 다시 추출한다.
            Write-Host "==> Ensuring dev signing key + embedded pubkey..."
            $Bash = Resolve-Bash
            if (-not $Bash) {
                Write-Error "Git Bash not found. Install Git for Windows to run scripts/gen-dev-key.sh."
                exit 1
            }
            & $Bash ./scripts/gen-dev-key.sh
            if ($LASTEXITCODE -ne 0) {
                Write-Error "gen-dev-key.sh failed with exit code $LASTEXITCODE"
                exit 1
            }
            $SignKeyPath = $DevKey
        }
    }
    $env:SIGN_KEY_PATH = $SignKeyPath
}

# 번들 플러그인은 여기서 빌드하지 않는다. 잠금 파일의 bundle = true 산출물을 받아 넣는다.
# 받기·SHA-256 대조·tar.gz 풀기·매니페스트 검증은 scripts/fetch-official-plugins.sh가 Git Bash에서 한다.
# 산출물이 없거나 해시가 다르면 실패하고 폴더를 남기지 않는다. 셸 쪽 규칙: scripts/lib/bundled-plugins.sh.
$BundledPluginsDir = Join-Path "target" "bundled-plugins"
$PluginPlatform = "windows-$Arch"
function Get-BundledPlugins {
    $Bash = Resolve-Bash
    if (-not $Bash) {
        Write-Error "Git Bash not found. Install Git for Windows to run scripts/fetch-official-plugins.sh."
        exit 1
    }
    # 이전 실행의 폴더가 있으면 unpack이 거부하므로 먼저 지운다.
    if (Test-Path $BundledPluginsDir) { Remove-Item -Recurse -Force $BundledPluginsDir }
    Write-Host "==> Fetching locked official plugins ($PluginPlatform)..."
    & $Bash ./scripts/fetch-official-plugins.sh --platform $PluginPlatform --out target/bundled-plugins
    if ($LASTEXITCODE -ne 0) {
        if (Test-Path $BundledPluginsDir) { Remove-Item -Recurse -Force $BundledPluginsDir }
        Write-Error "fetch-official-plugins.sh failed with exit code $LASTEXITCODE"
        exit 1
    }
}

# 받기가 실패하면 본체를 빌드하기 전에 멈춘다.
Get-BundledPlugins

Write-Host "==> Building tasty ($BuildProfile)..."
cargo build @CargoFlags
if ($LASTEXITCODE -ne 0) {
    Write-Error "cargo build failed with exit code $LASTEXITCODE"
    exit 1
}

# 매니페스트 서명은 이 함수 하나가 맡는다. 서명 주체가 바뀌면 이 함수만 바꾼다.
function Sign-BundledPlugins {
    $SignKeyPath = $env:SIGN_KEY_PATH
    $Bash = Resolve-Bash
    if (-not $Bash) {
        Write-Error "Git Bash not found. Install Git for Windows to run scripts/sign-bundle.sh."
        exit 1
    }
    Write-Host "==> Signing plugin manifests with $SignKeyPath..."
    # 받은 산출물에 .sig가 있으면 sign-bundle.sh가 실패한다(공식 플러그인은 서명 없이 온다).
    & $Bash ./scripts/sign-bundle.sh --key $SignKeyPath --dir target/bundled-plugins
    if ($LASTEXITCODE -ne 0) {
        Write-Error "sign-bundle.sh failed with exit code $LASTEXITCODE"
        exit 1
    }
}

if ($BuildProfile -ne "debug") {
    Sign-BundledPlugins
}

# 받은 <id>\ 폴더를 통째로 복사한다. 고지 파일(LICENSE 등)도 플러그인과 함께 간다.
function Stage-Plugins {
    param([string]$PluginsDir)
    New-Item -ItemType Directory -Force -Path $PluginsDir | Out-Null
    Copy-Item -Recurse -Force -Path (Join-Path $BundledPluginsDir "*") -Destination $PluginsDir
}

# 폴더 아래 파일의 상대 경로(\ 구분)를 낸다.
function Get-BundledPluginFiles([string]$Root) {
    $base = (Resolve-Path $Root).Path.TrimEnd('\') + '\'
    return @(Get-ChildItem -Path $Root -Recurse -File -Force | ForEach-Object { $_.FullName.Substring($base.Length) })
}

# 풀린 트리의 plugins 폴더가 스테이징 폴더와 파일 집합·SHA256 모두 같은지 본다.
function Test-BundledPluginTree([string]$Root, [string]$Label) {
    if (-not (Test-Path $Root)) {
        Write-Error "$Label has no plugins folder ($Root)"
        exit 1
    }
    $want = Get-BundledPluginFiles $BundledPluginsDir
    $got = Get-BundledPluginFiles $Root
    $missing = @($want | Where-Object { $got -notcontains $_ })
    $extra = @($got | Where-Object { $want -notcontains $_ })
    if ($missing.Count -gt 0 -or $extra.Count -gt 0) {
        Write-Error "bundled plugins in $Label differ from $BundledPluginsDir; missing: $($missing -join ', '); extra: $($extra -join ', ')"
        exit 1
    }
    foreach ($f in $want) {
        $a = (Get-FileHash (Join-Path $BundledPluginsDir $f) -Algorithm SHA256).Hash
        $b = (Get-FileHash (Join-Path $Root $f) -Algorithm SHA256).Hash
        if ($a -ne $b) {
            Write-Error "$f in $Label differs from $BundledPluginsDir"
            exit 1
        }
    }
    Write-Host "  plugins $Label :: $($want.Count) files match $BundledPluginsDir"
}

# WiX는 파일을 하나씩 적어야 하므로 받은 폴더에서 플러그인 fragment를 만든다. main.wxs의 PluginsDir 아래에
# 놓이고 Feature가 ComponentGroupRef BundledPlugins로 담는다. Id는 상대 경로의 SHA-256 앞 24자리라
# 같은 폴더 구성이면 같은 Id가 나온다. Source는 저장소 루트 기준 상대 경로다(cargo wix가 루트에서 실행).
$BundledPluginsWxs = Join-Path "target" "bundled-plugins.wxs"
$BundledPluginsSourcePrefix = "target\bundled-plugins\"
function Get-WixId([string]$Prefix, [string]$Rel) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Rel.ToLowerInvariant()))
    } finally {
        $sha.Dispose()
    }
    return $Prefix + (($bytes[0..11] | ForEach-Object { $_.ToString("x2") }) -join "")
}
function Add-WixDirectory($Lines, $Refs, [string]$Full, [string]$Rel, [string]$Indent) {
    foreach ($f in @(Get-ChildItem -Path $Full -File -Force | Sort-Object Name)) {
        $fileRel = if ($Rel) { "$Rel\$($f.Name)" } else { $f.Name }
        $cid = Get-WixId "bpc" $fileRel
        $fid = Get-WixId "bpf" $fileRel
        $name = [System.Security.SecurityElement]::Escape($f.Name)
        $src = [System.Security.SecurityElement]::Escape($BundledPluginsSourcePrefix + $fileRel)
        $Lines.Add("$Indent<Component Id='$cid' Guid='*'>") | Out-Null
        $Lines.Add("$Indent    <File Id='$fid' Name='$name' DiskId='1' Source='$src' KeyPath='yes'/>") | Out-Null
        $Lines.Add("$Indent</Component>") | Out-Null
        $Refs.Add($cid) | Out-Null
    }
    foreach ($d in @(Get-ChildItem -Path $Full -Directory -Force | Sort-Object Name)) {
        $dirRel = if ($Rel) { "$Rel\$($d.Name)" } else { $d.Name }
        $did = Get-WixId "bpd" $dirRel
        $name = [System.Security.SecurityElement]::Escape($d.Name)
        $Lines.Add("$Indent<Directory Id='$did' Name='$name'>") | Out-Null
        Add-WixDirectory $Lines $Refs $d.FullName $dirRel ($Indent + "    ")
        $Lines.Add("$Indent</Directory>") | Out-Null
    }
}
function New-BundledPluginsWxs {
    $lines = New-Object System.Collections.ArrayList
    $refs = New-Object System.Collections.ArrayList
    $lines.Add("<?xml version='1.0' encoding='utf-8'?>") | Out-Null
    $lines.Add("<!-- Generated by scripts/build-windows.ps1 from target\bundled-plugins. Do not edit. -->") | Out-Null
    $lines.Add("<Wix xmlns='http://schemas.microsoft.com/wix/2006/wi'>") | Out-Null
    $lines.Add("    <Fragment>") | Out-Null
    $lines.Add("        <DirectoryRef Id='PluginsDir'>") | Out-Null
    Add-WixDirectory $lines $refs (Resolve-Path $BundledPluginsDir).Path "" "            "
    $lines.Add("        </DirectoryRef>") | Out-Null
    $lines.Add("        <ComponentGroup Id='BundledPlugins'>") | Out-Null
    foreach ($r in $refs) { $lines.Add("            <ComponentRef Id='$r'/>") | Out-Null }
    $lines.Add("        </ComponentGroup>") | Out-Null
    $lines.Add("    </Fragment>") | Out-Null
    $lines.Add("</Wix>") | Out-Null
    [System.IO.File]::WriteAllText((Join-Path (Get-Location).Path $BundledPluginsWxs), (($lines -join "`r`n") + "`r`n"), (New-Object System.Text.UTF8Encoding $false))
    Write-Host "  wrote $BundledPluginsWxs ($($refs.Count) plugin files)"
}

# notice 파일은 LICENSE·THIRD_PARTY_LICENSES.md·LICENSES 바로 아래 파일이다. Linux/macOS와 같은 목록을 사용한다.
# LICENSES는 일반 파일만 담는다. 폴더·링크를 건너뛰면 그 본문이 빠지므로 scripts/lib/notice-set.sh처럼 실패한다.
# -Force로 Hidden 항목도 열거하고, 점으로 시작하는 이름은 셸과 같이 거부한다.
function Get-NoticeSetFiles {
    $files = @("LICENSE", "THIRD_PARTY_LICENSES.md")
    $entries = @(Get-ChildItem -Force -Path "LICENSES" | Sort-Object Name)
    foreach ($e in $entries) {
        if ($e.Name.StartsWith(".")) {
            Write-Error "LICENSES\$($e.Name) is a hidden entry; LICENSES\ holds licence texts only"
            exit 1
        }
        if ($e.PSIsContainer -or ($e.Attributes -band [System.IO.FileAttributes]::ReparsePoint)) {
            Write-Error "LICENSES\$($e.Name) is not a regular file; LICENSES\ holds licence texts only"
            exit 1
        }
    }
    $texts = $entries
    if ($texts.Count -eq 0) {
        Write-Error "LICENSES\ holds no licence text"
        exit 1
    }
    foreach ($t in $texts) { $files += (Join-Path "LICENSES" $t.Name) }
    return $files
}

# notice 문서의 상대 링크가 맞도록 LICENSES 디렉터리를 유지한다.
function Stage-Notice([string]$Dest) {
    New-Item -ItemType Directory -Force -Path (Join-Path $Dest "LICENSES") | Out-Null
    foreach ($f in Get-NoticeSetFiles) {
        Copy-Item $f -Destination (Join-Path $Dest $f)
    }
}

# SHA256으로 배포 사본을 원본과 대조한다.
function Test-NoticeTree([string]$Root, [string]$Label) {
    foreach ($f in Get-NoticeSetFiles) {
        $staged = Join-Path $Root $f
        if (-not (Test-Path $staged)) {
            Write-Error "$f missing from $Label"
            exit 1
        }
        $want = (Get-FileHash $f -Algorithm SHA256).Hash
        $got = (Get-FileHash $staged -Algorithm SHA256).Hash
        if ($want -ne $got) {
            Write-Error "$f in $Label differs from the repo copy"
            exit 1
        }
    }
}

Write-Host "==> Assembling archive..."
if (Test-Path $StageDir) { Remove-Item -Recurse -Force $StageDir }
New-Item -ItemType Directory -Force -Path $StageDir | Out-Null

$ExePath = Join-Path $BuildDir "tasty.exe"
if (-not (Test-Path $ExePath)) {
    Write-Error "Build output not found: $ExePath"
    exit 1
}
Copy-Item $ExePath -Destination $StageDir

Get-ChildItem -Path $BuildDir -Filter "*.dll" | ForEach-Object {
    Copy-Item $_.FullName -Destination $StageDir
}

Stage-Plugins (Join-Path $StageDir "plugins")
Stage-Notice $StageDir

Write-Host "==> Creating $ArchiveName..."
$ArchivePath = Join-Path $DistDir $ArchiveName
if (Test-Path $ArchivePath) { Remove-Item -Force $ArchivePath }
Compress-Archive -Path (Join-Path $StageDir "*") -DestinationPath $ArchivePath

Remove-Item -Recurse -Force $StageDir

Write-Host ""
Write-Host "Portable archive: $DistDir\$ArchiveName"

# MSI의 plugin 파일 목록은 아래 New-BundledPluginsWxs가 받은 폴더에서 만든다.
if (-not $SkipMsi) {
    Write-Host ""
    Write-Host "==> Building MSI installer..."

    if ($BuildProfile -eq "debug") {
        Write-Host "  (skipping MSI for debug build)"
    } else {
        $cargoWix = Get-Command cargo-wix -ErrorAction SilentlyContinue
        if (-not $cargoWix) {
            Write-Host "==> cargo-wix not found — installing via 'cargo install cargo-wix'..."
            cargo install cargo-wix
            if ($LASTEXITCODE -ne 0) {
                Write-Error "cargo install cargo-wix failed with exit code $LASTEXITCODE"
                exit 1
            }
            $cargoWix = Get-Command cargo-wix -ErrorAction SilentlyContinue
            if (-not $cargoWix) {
                Write-Error "cargo-wix still not on PATH after install. Ensure ~\.cargo\bin is on PATH, then re-run."
                exit 1
            }
        }

        # cargo-wix가 candle/light를 찾도록 WiX 설치 위치의 bin을 PATH에 추가한다.
        if (-not (Get-Command candle.exe -ErrorAction SilentlyContinue)) {
            $wixRoot = $env:WIX
            if (-not $wixRoot) {
                $wixRoot = [Environment]::GetEnvironmentVariable("WIX", "Machine")
            }
            $wixBin = if ($wixRoot) { Join-Path $wixRoot "bin" } else { $null }

            # WiX 설치는 관리자 자식 PowerShell에서 요청하며 UAC 승인이 필요할 수 있다.
            if (-not ($wixBin -and (Test-Path (Join-Path $wixBin "candle.exe")))) {
                Write-Host "==> WiX Toolset not found — installing via winget (a UAC admin prompt will appear)..."
                $winget = Get-Command winget -ErrorAction SilentlyContinue
                if (-not $winget) {
                    Write-Error "winget not found. Install WiX 3.x manually: winget install -e --id WiXToolset.WiXToolset"
                    exit 1
                }
                $elevCmd = 'Enable-WindowsOptionalFeature -Online -FeatureName NetFx3 -All -NoRestart -ErrorAction SilentlyContinue | Out-Null; winget install -e --id WiXToolset.WiXToolset --accept-source-agreements --accept-package-agreements; exit $LASTEXITCODE'
                try {
                    $proc = Start-Process -FilePath "powershell.exe" -Verb RunAs -Wait -PassThru `
                        -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $elevCmd)
                } catch {
                    Write-Error "Could not elevate to install WiX (UAC declined or unavailable): $($_.Exception.Message). Install manually: winget install -e --id WiXToolset.WiXToolset"
                    exit 1
                }
                # 설치 종료 코드만으로 중단하지 않고 아래에서 candle.exe를 다시 확인한다.
                if ($proc.ExitCode -ne 0) {
                    Write-Warning "Elevated WiX install returned exit code $($proc.ExitCode) — verifying candle.exe anyway..."
                }
                $wixRoot = [Environment]::GetEnvironmentVariable("WIX", "Machine")
                if (-not $wixRoot) {
                    $wixRoot = [Environment]::GetEnvironmentVariable("WIX", "User")
                }
                $wixBin = if ($wixRoot) { Join-Path $wixRoot "bin" } else { $null }
            }

            if ($wixBin -and (Test-Path (Join-Path $wixBin "candle.exe"))) {
                $env:Path = "$wixBin;$env:Path"
                Write-Host "  (added $wixBin to PATH for cargo-wix)"
            } else {
                Write-Error "WiX candle.exe not found after install. The WIX env var is set at machine scope on install — open a new shell and re-run so it propagates."
                exit 1
            }
        }

        # WiX 3은 3.14부터 ARM64 패키지(candle -arch arm64)를 만든다.
        if ($Arch -eq "arm64") {
            $CandleBanner = (& candle.exe -? 2>&1 | Select-Object -First 3) -join " "
            if ($CandleBanner -match 'version (\d+)\.(\d+)') {
                if ([int]$Matches[1] -lt 3 -or ([int]$Matches[1] -eq 3 -and [int]$Matches[2] -lt 14)) {
                    Write-Error "WiX $($Matches[1]).$($Matches[2]) cannot build ARM64 MSI packages; install WiX 3.14 or later."
                    exit 1
                }
            } else {
                Write-Warning "Could not read the WiX version from candle.exe; continuing."
            }
        }

        New-BundledPluginsWxs

        # WiX는 파일을 명시하므로 notice 목록의 Source 항목이 선언됐는지 먼저 확인한다.
        $WxsContent = Get-Content (Join-Path "wix" "main.wxs") -Raw
        foreach ($f in Get-NoticeSetFiles) {
            if (-not $WxsContent.Contains("Source='$f'")) {
                Write-Error "wix\main.wxs has no File with Source='$f' (notice set, see THIRD_PARTY_LICENSES.md)"
                exit 1
            }
        }

        if ($cargoWix) {
            $WixArgs = @(
                "wix",
                "--package", "tasty",
                "--profile", $BuildProfile,
                "--target", $Triple,
                "--no-build",
                "--nocapture",
                "--include", $BundledPluginsWxs,
                "--output", (Join-Path $DistDir $MsiName)
            )
            cargo @WixArgs
            if ($LASTEXITCODE -ne 0) {
                Write-Error "cargo wix failed with exit code $LASTEXITCODE"
                exit 1
            }
            Write-Host "Installer: $DistDir\$MsiName"
        }
    }
}

# 헤드리스는 gui feature를 끈 빌드라 별도 target 디렉터리를 쓴다. 같은 target을 쓰면
# 위 ZIP·MSI가 읽은 target\<triple>\<profile>\tasty.exe를 헤드리스 바이너리로 덮는다.
# plugin은 헤드리스 호스트도 실행하므로 위에서 받아 서명한 번들을 그대로 넣는다.
$HeadlessTargetDir = Join-Path "target" "headless"
$HeadlessStageDir = Join-Path $DistDir "tasty-headless-windows-${Arch}"
Write-Host "==> Building headless tasty ($BuildProfile, --no-default-features)..."
$PrevCargoTargetDir = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = $HeadlessTargetDir
try {
    cargo build @CargoFlags --no-default-features
    $HeadlessBuildExit = $LASTEXITCODE
} finally {
    $env:CARGO_TARGET_DIR = $PrevCargoTargetDir
}
if ($HeadlessBuildExit -ne 0) {
    Write-Error "cargo build (headless) failed with exit code $HeadlessBuildExit"
    exit 1
}

Write-Host "==> Assembling headless archive..."
if (Test-Path $HeadlessStageDir) { Remove-Item -Recurse -Force $HeadlessStageDir }
New-Item -ItemType Directory -Force -Path $HeadlessStageDir | Out-Null
$HeadlessBuildDir = Join-Path (Join-Path $HeadlessTargetDir $Triple) $BuildProfile
$HeadlessExePath = Join-Path $HeadlessBuildDir "tasty.exe"
if (-not (Test-Path $HeadlessExePath)) {
    Write-Error "Headless build output not found: $HeadlessExePath"
    exit 1
}
# 두 빌드의 feature가 다르므로 GUI 바이너리와 같은 파일이면 헤드리스 빌드가 적용되지 않은 것이다.
if ((Get-FileHash $HeadlessExePath -Algorithm SHA256).Hash -eq (Get-FileHash $ExePath -Algorithm SHA256).Hash) {
    Write-Error "$HeadlessArchiveName would hold the GUI binary"
    exit 1
}
Copy-Item $HeadlessExePath -Destination $HeadlessStageDir
Get-ChildItem -Path $HeadlessBuildDir -Filter "*.dll" | ForEach-Object {
    Copy-Item $_.FullName -Destination $HeadlessStageDir
}
Stage-Plugins (Join-Path $HeadlessStageDir "plugins")
Stage-Notice $HeadlessStageDir

Write-Host "==> Creating $HeadlessArchiveName..."
$HeadlessArchivePath = Join-Path $DistDir $HeadlessArchiveName
if (Test-Path $HeadlessArchivePath) { Remove-Item -Force $HeadlessArchivePath }
Compress-Archive -Path (Join-Path $HeadlessStageDir "*") -DestinationPath $HeadlessArchivePath
Remove-Item -Recurse -Force $HeadlessStageDir

# PE 헤더의 Machine 필드로 실행 파일의 아키텍처를 읽는다(x64 0x8664, ARM64 0xAA64).
function Get-PeMachine([string]$Path) {
    $fs = [System.IO.File]::OpenRead($Path)
    try {
        $br = New-Object System.IO.BinaryReader($fs)
        if ($fs.Length -lt 0x40 -or $br.ReadUInt16() -ne 0x5A4D) { return $null }
        $fs.Position = 0x3C
        $peOffset = $br.ReadInt32()
        if ($peOffset -lt 0 -or $peOffset + 6 -gt $fs.Length) { return $null }
        $fs.Position = $peOffset
        if ($br.ReadUInt32() -ne 0x00004550) { return $null }
        return [int]$br.ReadUInt16()
    } finally {
        $fs.Dispose()
    }
}

# 압축·설치 패키지를 푼 트리의 모든 .exe가 대상 아키텍처인지 확인하고 결과를 로그에 남긴다.
function Test-PeTree([string]$Root, [string]$Label, [int]$MinExe) {
    $exes = @(Get-ChildItem -Path $Root -Recurse -File -Filter "*.exe")
    if ($exes.Count -lt $MinExe) {
        Write-Error "$Label holds $($exes.Count) .exe files, expected at least $MinExe"
        exit 1
    }
    foreach ($e in $exes) {
        $m = Get-PeMachine $e.FullName
        $rel = $e.FullName.Substring((Resolve-Path $Root).Path.Length).TrimStart('\')
        $shown = if ($null -eq $m) { "not a PE file" } else { "0x{0:X4}" -f $m }
        Write-Host "  PE $Label :: $rel = $shown"
        if ($m -ne $ExpectedMachine) {
            Write-Error ("$Label :: $rel is $shown, expected 0x{0:X4} ($Arch)" -f $ExpectedMachine)
            exit 1
        }
    }
}

# 다른 아키텍처의 실행 파일은 실행할 수 없으므로 PE 머신 타입 검사로 대신한다.
function Test-Launch([string]$Exe, [string]$Label) {
    if (-not $CanRunTarget) {
        Write-Host "  skip $Label --version: $Arch binary cannot run on this $HostArch host (PE machine check covers the architecture)"
        return
    }
    & $Exe --version | Out-Null
    if ($LASTEXITCODE -ne 0) {
        Write-Error "$Label --version failed with exit code $LASTEXITCODE"
        exit 1
    }
    Write-Host "  ran $Label --version"
}

# tasty.exe와 번들 plugin 실행 파일 수. 스테이징 폴더는 잠금 파일의 이 플랫폼 bundle = true 계획과
# 같은 <id> 폴더만 담는다(받기 스크립트의 unpack이 다른 폴더를 거부한다). 고지 파일은 .exe가 아니라 세지 않는다.
$BundleExeCount = 1 + @(Get-ChildItem -Path $BundledPluginsDir -Directory).Count

Write-Host "==> Verifying artifacts ($Arch)..."
$VerifyDir = Join-Path $env:TEMP "tasty-verify-$([guid]::NewGuid())"
Expand-Archive -Path $ArchivePath -DestinationPath $VerifyDir
$VerifyExe = Join-Path $VerifyDir "tasty.exe"
if (-not (Test-Path $VerifyExe)) {
    Remove-Item -Recurse -Force $VerifyDir -ErrorAction SilentlyContinue
    Write-Error "tasty.exe not found in ZIP"
    exit 1
}
Test-PeTree $VerifyDir $ArchiveName $BundleExeCount
Test-Launch $VerifyExe "$ArchiveName tasty.exe"
Test-NoticeTree $VerifyDir $ArchiveName
Test-BundledPluginTree (Join-Path $VerifyDir "plugins") $ArchiveName
Remove-Item -Recurse -Force $VerifyDir -ErrorAction SilentlyContinue

$HeadlessVerifyDir = Join-Path $env:TEMP "tasty-verify-headless-$([guid]::NewGuid())"
Expand-Archive -Path $HeadlessArchivePath -DestinationPath $HeadlessVerifyDir
$HeadlessVerifyExe = Join-Path $HeadlessVerifyDir "tasty.exe"
if (-not (Test-Path $HeadlessVerifyExe)) {
    Remove-Item -Recurse -Force $HeadlessVerifyDir -ErrorAction SilentlyContinue
    Write-Error "tasty.exe not found in $HeadlessArchiveName"
    exit 1
}
if (-not (Test-Path (Join-Path $HeadlessVerifyDir "plugins"))) {
    Remove-Item -Recurse -Force $HeadlessVerifyDir -ErrorAction SilentlyContinue
    Write-Error "plugins not found in $HeadlessArchiveName"
    exit 1
}
Test-PeTree $HeadlessVerifyDir $HeadlessArchiveName $BundleExeCount
Test-Launch $HeadlessVerifyExe "$HeadlessArchiveName tasty.exe"
Test-NoticeTree $HeadlessVerifyDir $HeadlessArchiveName
Test-BundledPluginTree (Join-Path $HeadlessVerifyDir "plugins") $HeadlessArchiveName
Remove-Item -Recurse -Force $HeadlessVerifyDir -ErrorAction SilentlyContinue

$BuildsMsi = (-not $SkipMsi -and $BuildProfile -ne "debug")
$MsiPath = Join-Path $DistDir $MsiName
if ($BuildsMsi) {
    if (-not (Test-Path $MsiPath)) {
        Write-Error "MSI missing: $MsiPath"
        exit 1
    }
    $MsiExtract = Join-Path $env:TEMP "tasty-msi-$([guid]::NewGuid())"
    if ($CanRunTarget) {
        # 관리 설치로 MSI를 별도 디렉터리에 풀어 설치 트리 그대로 대조한다.
        $msiProc = Start-Process -FilePath "msiexec.exe" -Wait -PassThru `
            -ArgumentList @("/a", "`"$((Resolve-Path $MsiPath).Path)`"", "/qn", "TARGETDIR=`"$MsiExtract`"")
        if ($msiProc.ExitCode -ne 0) {
            Remove-Item -Recurse -Force $MsiExtract -ErrorAction SilentlyContinue
            Write-Error "msiexec /a failed with exit code $($msiProc.ExitCode)"
            exit 1
        }
        # 플러그인 폴더는 자기 고지 파일을 담을 수 있으므로 루트 인벤토리는 bin\plugins 밖에서 센다.
        $inventory = @(Get-ChildItem -Path $MsiExtract -Recurse -File -Filter "THIRD_PARTY_LICENSES.md" |
            Where-Object { $_.FullName -notmatch '\\bin\\plugins\\' })
        if ($inventory.Count -ne 1) {
            Remove-Item -Recurse -Force $MsiExtract -ErrorAction SilentlyContinue
            Write-Error "expected one THIRD_PARTY_LICENSES.md outside bin\plugins in the MSI, found $($inventory.Count)"
            exit 1
        }
        Test-NoticeTree $inventory[0].DirectoryName $MsiName
        Test-PeTree $MsiExtract $MsiName $BundleExeCount
        $msiExe = @(Get-ChildItem -Path $MsiExtract -Recurse -File -Filter "tasty.exe")
        if ($msiExe.Count -ne 1) {
            Remove-Item -Recurse -Force $MsiExtract -ErrorAction SilentlyContinue
            Write-Error "expected one tasty.exe in the MSI, found $($msiExe.Count)"
            exit 1
        }
        Test-BundledPluginTree (Join-Path $msiExe[0].DirectoryName "plugins") $MsiName
    } else {
        # 다른 아키텍처의 MSI는 이 호스트에서 관리 설치할 수 없으므로 WiX dark로 cabinet의 파일을 꺼낸다.
        # 꺼낸 파일은 File Id 이름이라 main.wxs와 플러그인 fragment의 Id·Source 짝으로 상대 경로에 다시 놓는다.
        # 플러그인 파일은 plugins\<id>\... 로 놓아 받은 폴더와 대조한다.
        $Dark = Get-Command dark.exe -ErrorAction SilentlyContinue
        if (-not $Dark) {
            Write-Error "dark.exe not found next to candle.exe; cannot inspect the $Arch MSI on this $HostArch host"
            exit 1
        }
        $DarkOut = Join-Path $MsiExtract "dark"
        & $Dark.Source -nologo -x $DarkOut (Resolve-Path $MsiPath).Path (Join-Path $MsiExtract "decompiled.wxs")
        if ($LASTEXITCODE -ne 0) {
            Remove-Item -Recurse -Force $MsiExtract -ErrorAction SilentlyContinue
            Write-Error "dark.exe failed with exit code $LASTEXITCODE"
            exit 1
        }
        $MsiTree = Join-Path $MsiExtract "tree"
        $WxsText = (Get-Content (Join-Path "wix" "main.wxs") -Raw) + (Get-Content $BundledPluginsWxs -Raw)
        foreach ($fileElement in [regex]::Matches($WxsText, '<File\b[^>]*>')) {
            $id = [regex]::Match($fileElement.Value, "\bId='([^']+)'").Groups[1].Value
            $src = [regex]::Match($fileElement.Value, "\bSource='([^']+)'").Groups[1].Value
            if (-not $id -or -not $src) { continue }
            $rel = $src -replace '^\$\(var\.CargoTargetBinDir\)\\', ''
            if ($rel.StartsWith($BundledPluginsSourcePrefix)) {
                $rel = Join-Path "plugins" $rel.Substring($BundledPluginsSourcePrefix.Length)
            }
            $extracted = Join-Path (Join-Path $DarkOut "File") $id
            if (-not (Test-Path $extracted)) {
                $found = @(Get-ChildItem -Path $DarkOut -Recurse -File | Select-Object -First 5 | ForEach-Object { $_.FullName })
                Remove-Item -Recurse -Force $MsiExtract -ErrorAction SilentlyContinue
                Write-Error "File Id $id ($src) was not extracted from $MsiName; sample of extracted files: $($found -join ', ')"
                exit 1
            }
            $dest = Join-Path $MsiTree $rel
            New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dest) | Out-Null
            Copy-Item $extracted -Destination $dest
        }
        Test-NoticeTree $MsiTree $MsiName
        Test-PeTree $MsiTree $MsiName $BundleExeCount
        Test-BundledPluginTree (Join-Path $MsiTree "plugins") $MsiName
    }
    Remove-Item -Recurse -Force $MsiExtract -ErrorAction SilentlyContinue
}

$ShaSumsPath = Join-Path $DistDir $ShaSumsName
$Distributables = @($ArchivePath, $HeadlessArchivePath)
if ($BuildsMsi) { $Distributables += $MsiPath }
$Sums = @()
foreach ($d in $Distributables) {
    $Sums += "$((Get-FileHash $d -Algorithm SHA256).Hash.ToLower())  $(Split-Path -Leaf $d)"
}
$Sums | Out-File -Encoding ASCII $ShaSumsPath

# 체크섬 파일을 다시 읽어 이 아키텍처의 배포물 전부를 담는지와 각 해시가 맞는지 확인한다.
Write-Host "==> Verifying $ShaSumsName..."
$Listed = @{}
foreach ($line in Get-Content $ShaSumsPath) {
    if ($line -notmatch '^([0-9a-f]{64})  (.+)$') {
        Write-Error "malformed line in ${ShaSumsName}: $line"
        exit 1
    }
    $Listed[$Matches[2]] = $Matches[1]
}
$Expected = @($Distributables | ForEach-Object { Split-Path -Leaf $_ })
if ($Listed.Count -ne $Expected.Count) {
    Write-Error "$ShaSumsName lists $($Listed.Count) files, expected $($Expected.Count): $($Expected -join ', ')"
    exit 1
}
foreach ($name in $Expected) {
    if (-not $Listed.ContainsKey($name)) {
        Write-Error "$ShaSumsName does not list $name"
        exit 1
    }
    $actual = (Get-FileHash (Join-Path $DistDir $name) -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $Listed[$name]) {
        Write-Error "$name does not match its SHA-256 in $ShaSumsName"
        exit 1
    }
    Write-Host "  sha256 ok $name"
}

# 이 실행이 만든 산출물 명부. staging 디렉터리가 남지 않았는지도 확인한다.
Write-Host "==> $Arch artifacts in ${DistDir}:"
foreach ($name in ($Expected + $ShaSumsName)) {
    $item = Get-Item (Join-Path $DistDir $name)
    Write-Host ("  {0}  {1} bytes" -f $name, $item.Length)
}
foreach ($stage in @($StageDir, $HeadlessStageDir)) {
    if (Test-Path $stage) {
        Write-Error "staging directory left behind: $stage"
        exit 1
    }
}

Write-Host ""
Write-Host "Done!"
Write-Host "SHA: $ShaSumsPath"

} finally {
    Pop-Location
}
