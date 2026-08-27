# Builds and runs the experiment.
#
# rustup installs the MSVC toolchain but does not set up the MSVC/Windows SDK
# library search paths, so we import them from vcvars64.bat first.
#
# Positional binding is off: without it `-Machine serve` hands "serve" to
# whichever [string] parameter happens to be declared first, which is a very
# confusing way to be told nothing is wrong.
[CmdletBinding(PositionalBinding = $false)]
param(
    [switch]$Test,
    [switch]$BuildOnly,
    [switch]$Serve,
    # Experiments 06 and 07: the machine designer, which is a separate binary
    # with a separate front end. Everything after the switch is passed straight
    # to it:
    #   -Machine                       every design in ./designs, judged by brief
    #   -Machine serve                 the designer, in a browser
    #   -Machine run designs/x.machine
    #   -Machine why designs/x.machine what every component is doing, and why
    #   -Machine parts [family]        the vocabulary, with its constraints
    #   -Machine reuse                 which primitives earned their place
    #   -Machine check                 the front end, against a live server
    # Experiment 08 turns the same document into a plant:
    #   -Machine form designs/x.machine [--png shot.png] [--obj plant.obj]
    #   -Machine forms                 every design built, counted and hashed
    #   -Machine kit [--png sheet.png] the asset library, one of everything
    # Experiment 09 builds it four ways and compares them:
    #   -Machine read designs/x.machine [--png sheet.png]
    #   -Machine reads                 every design, at all four grades
    # Experiment 10 hands the third dimension to the player and reports on it:
    #   -Machine space designs/x.machine   placement, ports, routing, clashes
    #   -Machine spaces                    every design, routed and judged
    # Anything that builds a plant takes --grade a|b|c|d as well as --style and
    # --seed.
    [switch]$Machine,
    [string]$Play,
    # PowerShell binds anything starting with `-` to a parameter, so the
    # scenario's own options get first-class ones rather than being smuggled
    # through the remaining arguments.
    [string[]]$Buy,
    [long]$At = 0,
    [int]$Port = 8787,
    [Parameter(ValueFromRemainingArguments)]$Configs
)

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
if ($Machine) {
    cargo build --release --bin machine
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $exe = Join-Path $PSScriptRoot "target/release/machine.exe"
    # `check` is the front end's own test, and it needs something to talk to.
    # Starting and stopping that here is the difference between a test people
    # run and a test people mean to run.
    if ($Configs -and $Configs[0] -eq "check") {
        $port = if ($Port -eq 8787) { 8799 } else { $Port }
        $srv = Start-Process -FilePath $exe -ArgumentList "serve", "--port", $port `
            -WorkingDirectory $PSScriptRoot -PassThru -WindowStyle Hidden
        try {
            Start-Sleep -Milliseconds 700
            node (Join-Path $PSScriptRoot "tests\machine_web.mjs") $port
            exit $LASTEXITCODE
        } finally {
            if (-not $srv.HasExited) { Stop-Process -Id $srv.Id -Force }
        }
    }
    $cli = @()
    if ($Configs) { $cli += $Configs }
    if ($cli -contains "serve" -and $Port -ne 8787) { $cli += @("--port", $Port) }
    & $exe @cli
    exit $LASTEXITCODE
} elseif ($Test) {
    cargo test --release
} elseif ($BuildOnly) {
    cargo build --release
} elseif ($Serve) {
    # The workbench. Built first so a compile error is a compile error rather
    # than a browser that will not connect.
    cargo build --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    & "$PSScriptRoot\target\release\trooms.exe" serve --port $Port
} elseif ($Play) {
    # A scenario, played headlessly: the brief, why every class is stopped,
    # what is binding, whether the order was met, and the replay check.
    cargo build --release
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $opts = @()
    foreach ($b in $Buy) { $opts += @("--buy", $b) }
    if ($At -gt 0) { $opts += @("--at", $At) }
    & "$PSScriptRoot\target\release\trooms.exe" play $Play @opts
} elseif ($Configs) {
    cargo run --release -- @Configs
} else {
    cargo run --release
}
