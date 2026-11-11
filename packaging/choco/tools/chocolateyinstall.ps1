$ErrorActionPreference = 'Stop'
$version = '0.44.0'
$packageArgs = @{
  packageName   = $env:ChocolateyPackageName
  unzipLocation = (Split-Path -Parent $MyInvocation.MyCommand.Definition)
  url64bit      = "https://github.com/kjmcnamara1/cecilia/releases/download/v$version/cecilia-v$version-x86_64-pc-windows-msvc.zip"
  checksum64    = '0000000000000000000000000000000000000000000000000000000000000000'
  checksumType64 = 'sha256'
}
# The zip holds one folder with cecilia.exe; Chocolatey makes a shim for every .exe it unpacks.
Install-ChocolateyZipPackage @packageArgs
