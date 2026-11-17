param([string]$Destination = (Join-Path $env:LOCALAPPDATA 'MarkdownDev'))
$ErrorActionPreference = 'Stop'
# A parent pwsh can pass its module search path to Windows PowerShell. Load the
# matching built-in utility module explicitly, including its Get-FileHash helper.
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
$target = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Destination).TrimEnd('\')
if (Test-Path -LiteralPath $target) { throw 'Choose a new installation directory; existing paths are never overwritten.' }
if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'Markdown.Windows.exe'))) { throw 'Run install.ps1 from the extracted Markdown package.' }
$source = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\') + '\'
if ($target.StartsWith($source, [StringComparison]::OrdinalIgnoreCase)) { throw 'Install outside the extracted package.' }
# Do not follow junctions or symlinks supplied in a package or destination ancestry.
$ancestor = [IO.DirectoryInfo]$target
while ($null -ne $ancestor) {
    if ($ancestor.Exists -and ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse-point destinations are unsupported.' }
    $ancestor = $ancestor.Parent
}
$items = @(Get-ChildItem -LiteralPath $PSScriptRoot -Recurse -Force)
if ($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'Reparse-point payloads are unsupported.' }
# Stage beside the requested destination; publish only a complete installation.
$parentPath = Split-Path -Parent $target
if (-not (Test-Path -LiteralPath $parentPath -PathType Container)) { throw 'The installation parent directory must exist.' }
$stageName = '.markdown-install-' + [Guid]::NewGuid().ToString('N')
$stage = [IO.Path]::GetFullPath((Join-Path $parentPath $stageName))
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    $files = @()
    foreach ($item in $items) {
        $relative = $item.FullName.Substring($source.Length)
        $output = Join-Path $stage $relative
        if ($item.PSIsContainer) { New-Item -ItemType Directory -Path $output -Force | Out-Null }
        else {
            $parent = Split-Path -Parent $output
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
            Copy-Item -LiteralPath $item.FullName -Destination $output
            $files += @{path=$relative; sha256=(Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash}
        }
    }
    @{product='markdown-development/v1'; destination=$target; files=$files} | ConvertTo-Json -Depth 5 |
        Set-Content -LiteralPath (Join-Path $stage '.markdown-dev-install.json') -Encoding UTF8
    [IO.Directory]::Move($stage, $target)
}
catch {
    # This unique staging directory is the only recursive cleanup target. Verify
    # its final absolute path, parent and non-reparse identity before removing it.
    if (Test-Path -LiteralPath $stage) {
        $resolvedStage = Get-Item -LiteralPath $stage -Force
        if ($resolvedStage.FullName -eq $stage -and $resolvedStage.Parent.FullName -eq $parentPath -and
            $resolvedStage.Name -eq $stageName -and -not ($resolvedStage.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            Remove-Item -LiteralPath $stage -Recurse -Force
        }
    }
    throw
}
Write-Output "Installed unsigned development app. Launch: $(Join-Path $target 'Markdown.Windows.exe')"
Write-Output 'No administrator rights, file associations or user documents were changed.'
