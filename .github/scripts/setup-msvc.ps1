$ErrorActionPreference = 'Stop'

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) {
    throw 'vswhere.exe is missing from the Windows runner'
}

$installation = & $vswhere -latest -products * `
    -requires `
    Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    Microsoft.VisualStudio.Component.VC.Llvm.Clang `
    -property installationPath
if ($LASTEXITCODE -ne 0 -or -not $installation) {
    throw 'Visual Studio C++ Build Tools are missing from the Windows runner'
}

$vcvars = Join-Path $installation 'VC\Auxiliary\Build\vcvars64.bat'
$libclang = Join-Path $installation 'VC\Tools\Llvm\x64\bin'
if (-not (Test-Path -LiteralPath $vcvars)) {
    throw 'vcvars64.bat is missing from the Windows runner'
}
if (-not (Test-Path -LiteralPath (Join-Path $libclang 'libclang.dll'))) {
    throw 'libclang.dll is missing from the Windows runner'
}

$toolsetsRoot = Join-Path $installation 'VC\Tools\MSVC'
$toolset = Get-ChildItem -LiteralPath $toolsetsRoot -Directory |
    Sort-Object { [version]$_.Name } -Descending |
    Where-Object {
        (Test-Path -LiteralPath (Join-Path $_.FullName 'bin\Hostx64\x64\cl.exe')) -and
        (Test-Path -LiteralPath (Join-Path $_.FullName 'lib\x64\libcmt.lib'))
    } |
    Select-Object -First 1
if (-not $toolset) {
    throw 'no complete x64 MSVC toolset is installed on the Windows runner'
}

$command = "call `"$vcvars`" -vcvars_ver=$($toolset.Name) >nul && set"
$lines = & $env:ComSpec /d /s /c $command
if ($LASTEXITCODE -ne 0) {
    throw 'vcvars64.bat failed to initialize the MSVC environment'
}

$captured = [System.Collections.Generic.Dictionary[string, string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)
foreach ($line in $lines) {
    $separator = $line.IndexOf('=')
    if ($separator -gt 0) {
        $captured[$line.Substring(0, $separator)] = $line.Substring($separator + 1)
    }
}

$required = @(
    'Path',
    'INCLUDE',
    'LIB',
    'LIBPATH',
    'UCRTVersion',
    'UniversalCRTSdkDir',
    'VCINSTALLDIR',
    'VCToolsInstallDir',
    'VSINSTALLDIR',
    'WindowsSdkDir',
    'WindowsSDKVersion'
)

function Export-BuildVariable {
    param(
        [Parameter(Mandatory)]
        [string] $Name,
        [Parameter(Mandatory)]
        [string] $Value
    )

    Set-Item -LiteralPath "Env:$Name" -Value $Value
    if ($env:GITHUB_ENV) {
        Add-Content -LiteralPath $env:GITHUB_ENV -Value "$Name=$Value" -Encoding utf8
    }
}

foreach ($name in $required) {
    if (-not $captured.ContainsKey($name)) {
        throw "vcvars64.bat did not define $name"
    }
    Export-BuildVariable -Name $name -Value $captured[$name]
}

Export-BuildVariable -Name 'LIBCLANG_PATH' -Value $libclang
Export-BuildVariable -Name 'CMAKE_GENERATOR' -Value 'Ninja'
Export-BuildVariable -Name 'CC' -Value 'cl'
Export-BuildVariable -Name 'CXX' -Value 'cl'

# Release binaries target the x86-64/SSE2 baseline guaranteed by MSVC instead
# of the build runner's CPU. whisper.cpp otherwise auto-detects the runner and
# can silently emit AVX2 instructions that fail on older Windows x64 systems.
$portableCpuOptions = [ordered]@{
    GGML_NATIVE = 'OFF'
    GGML_SSE42  = 'OFF'
    GGML_AVX    = 'OFF'
    GGML_AVX2   = 'OFF'
    GGML_BMI2   = 'OFF'
}
foreach ($option in $portableCpuOptions.GetEnumerator()) {
    Export-BuildVariable -Name $option.Key -Value $option.Value
}

$cmake = Get-Command cmake.exe -CommandType Application -ErrorAction SilentlyContinue
if (-not $cmake) {
    throw 'CMake 3.15 or newer is required for the static MSVC runtime build'
}
$cmakeOutput = @(& $cmake.Source --version)
$cmakeExitCode = $LASTEXITCODE
$cmakeBanner = $cmakeOutput | Select-Object -First 1
$cmakeMatch = [regex]::Match($cmakeBanner, 'cmake version (?<Version>\d+(?:\.\d+){1,3})')
if ($cmakeExitCode -ne 0 -or -not $cmakeMatch.Success) {
    throw 'CMake returned an unrecognized version'
}
$cmakeVersion = [version]$cmakeMatch.Groups['Version'].Value
if ($cmakeVersion -lt [version]'3.15') {
    throw "CMake 3.15 or newer is required; found $cmakeVersion"
}
if (-not (Get-Command ninja.exe -CommandType Application -ErrorAction SilentlyContinue)) {
    throw 'Ninja is required for the Windows build'
}
