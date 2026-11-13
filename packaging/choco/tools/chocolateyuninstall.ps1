$ErrorActionPreference = 'Stop'
# Chocolatey removes the unpacked files and the shims; the Start Menu shortcut is ours.
$shortcut = Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'cecilia.lnk'
if (Test-Path $shortcut) { Remove-Item $shortcut -Force }
