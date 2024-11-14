Installation as a Windows service:

```powershell
New-Service -Name "urad_ingester" -DisplayName "URadMonitor Ingester" -Description "URadMonitor data recorder and server" -StartupType Manual -BinaryPathName "C:\projects\urad-ingester\target\release\urad-ingester.exe"
```