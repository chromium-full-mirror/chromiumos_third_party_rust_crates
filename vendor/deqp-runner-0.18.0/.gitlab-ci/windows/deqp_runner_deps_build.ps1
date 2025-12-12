# we want more secure TLS 1.2 for most things
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12 -bor [Net.SecurityProtocolType]::Tls13;

# Download new TLS certs from Windows Update
Get-Date
Write-Host "Updating TLS certificate store"
$certdir = (New-Item -ItemType Directory -Name "_tlscerts")
certutil -syncwithWU "$certdir"
Foreach ($file in (Get-ChildItem -Path "$certdir\*" -Include "*.crt")) {
  Import-Certificate -FilePath $file -CertStoreLocation Cert:\LocalMachine\Root
}
Remove-Item -Recurse -Path $certdir

$url = 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe';
Write-Host ('Downloading {0} ...' -f $url);
Invoke-WebRequest -Uri $url -OutFile 'rustup-init.exe';

C:\rustup-init.exe -y;
Remove-Item C:\rustup-init.exe;

Get-Date
Write-Host "Complete"
