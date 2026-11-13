$ErrorActionPreference = 'Stop'
$version = '0.46.0'
$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  unzipLocation = (Split-Path -Parent $MyInvocation.MyCommand.Definition)
  url64bit      = "https://github.com/kjmcnamara1/cecilia/releases/download/v$version/cecilia-v$version-x86_64-pc-windows-msvc.zip"
  checksum64    = '0000000000000000000000000000000000000000000000000000000000000000'
  checksumType64 = 'sha256'
}
# The zip holds one folder with cecilia.exe and ceciliag.exe; Chocolatey makes a shim for every
# .exe it unpacks. ceciliag.exe is the window without a console: the .gui file makes its shim a
# GUI program too, and the Start Menu shortcut opens it.
Install-ChocolateyZipPackage @packageArgs
$dir = Join-Path $packageArgs.unzipLocation "cecilia-v$version-x86_64-pc-windows-msvc"
New-Item -ItemType File -Force -Path (Join-Path $dir 'ceciliag.exe.gui') | Out-Null
Install-ChocolateyShortcut `
  -ShortcutFilePath (Join-Path ([Environment]::GetFolderPath('CommonPrograms')) 'cecilia.lnk') `
  -TargetPath (Join-Path $dir 'ceciliag.exe')
