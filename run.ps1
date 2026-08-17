# Builds and runs the experiment.
#
# rustup installs the MSVC toolchain but does not set up the MSVC/Windows SDK
# library search paths, so we import them from vcvars64.bat first.
param([switch]$Test, [switch]$BuildOnly)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# This VS install ships vcvars64.bat but not the vcvarsall.bat it delegates to,
# so discover the x64 library directories ourselves.
$vswhere = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
$inst = & $vswhere -latest -products * -property installationPath
$toolset = Get-ChildItem (Join-Path $inst "VC\Tools\MSVC") |
    Sort-Object Name -Descending | Select-Object -First 1
$sdkRoot = "C:\Program Files (x86)\Windows Kits\10"
$sdkVer = Get-ChildItem (Join-Path $sdkRoot "Lib") |
    Sort-Object Name -Descending | Select-Object -First 1

# This toolset ships only the OneCore flavour of the CRT import libraries.
$crt = @("lib\x64", "lib\onecore\x64") |
    ForEach-Object { Join-Path $toolset.FullName $_ } |
    Where-Object { Test-Path (Join-Path $_ "msvcrt.lib") } |
    Select-Object -First 1
if (-not $crt) { throw "no x64 CRT import libraries under $($toolset.FullName)\lib" }

$libs = @(
    $crt,
    (Join-Path $sdkVer.FullName "ucrt\x64"),
    (Join-Path $sdkVer.FullName "um\x64")
)
foreach ($l in $libs) {
    if (-not (Test-Path $l)) { throw "missing library directory: $l" }
}
$env:LIB = ($libs -join ";")
$env:PATH = (Join-Path $toolset.FullName "bin\HostX64\x64") + ";$env:PATH"

Set-Location $PSScriptRoot
if ($Test) {
    cargo test --release
} elseif ($BuildOnly) {
    cargo build --release
} else {
    cargo run --release
}
