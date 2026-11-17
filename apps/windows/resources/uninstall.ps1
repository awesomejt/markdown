param()
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath($PSScriptRoot).TrimEnd('\')
$prefix = $target + '\'
$marker = Join-Path $target '.markdown-dev-install.json'
$record = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
if ($record.product -ne 'markdown-development/v1' -or $record.destination -ne $target) { throw 'Installation identity mismatch.' }
$ancestor = [IO.DirectoryInfo]$target
while ($null -ne $ancestor) {
    if ($ancestor.Exists -and ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Reparse-point installations are unsupported.' }
    $ancestor = $ancestor.Parent
}
if (Get-ChildItem -LiteralPath $target -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) {
    throw 'Remove reparse points before uninstalling.'
}
$owned = @()
foreach ($entry in $record.files) {
    if ([IO.Path]::IsPathRooted($entry.path) -or $entry.path.Contains(':')) { throw 'Invalid installed path.' }
    $file = [IO.Path]::GetFullPath((Join-Path $target $entry.path))
    if (-not $file.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { throw 'Installed path escapes its root.' }
    if (Test-Path -LiteralPath $file) {
        if ((Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $entry.sha256) { throw 'Installed file changed; no files removed.' }
        $owned += $file
    }
}
# All absolute file targets were checked above; never recursively delete the installation.
foreach ($file in $owned) { Remove-Item -LiteralPath $file }
Remove-Item -LiteralPath $marker
Get-ChildItem -LiteralPath $target -Directory -Recurse -Force | Sort-Object { $_.FullName.Length } -Descending | ForEach-Object {
    if (-not (Get-ChildItem -LiteralPath $_.FullName -Force)) { Remove-Item -LiteralPath $_.FullName }
}
if (-not (Get-ChildItem -LiteralPath $target -Force)) { Remove-Item -LiteralPath $target }
Write-Output 'Development app uninstalled. Preferences, recents, restoration and unowned files were preserved.'
