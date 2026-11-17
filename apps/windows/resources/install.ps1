param([string]$Destination = (Join-Path $env:LOCALAPPDATA 'MarkdownDev'))
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath($Destination)
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
New-Item -ItemType Directory -Path $target | Out-Null
$files = @()
foreach ($item in $items) {
    $relative = $item.FullName.Substring($source.Length)
    $output = Join-Path $target $relative
    if ($item.PSIsContainer) { New-Item -ItemType Directory -Path $output -Force | Out-Null }
    else {
        $parent = Split-Path -Parent $output
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
        Copy-Item -LiteralPath $item.FullName -Destination $output
        $files += @{path=$relative; sha256=(Get-FileHash -LiteralPath $output -Algorithm SHA256).Hash}
    }
}
@{product='markdown-development/v1'; destination=$target; files=$files} | ConvertTo-Json -Depth 5 |
    Set-Content -LiteralPath (Join-Path $target '.markdown-dev-install.json') -Encoding UTF8
Write-Output "Installed unsigned development app. Launch: $(Join-Path $target 'Markdown.Windows.exe')"
Write-Output 'No administrator rights, file associations or user documents were changed.'
