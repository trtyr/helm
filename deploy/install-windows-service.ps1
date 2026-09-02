# 安装 helm-agent 为 Windows 服务（去黑窗口的 GUI 子系统二进制 + nssm 包装）。
# 前提：已安装 nssm（https://nssm.cc），且以管理员身份运行。
#
# 用法（管理员 PowerShell）：
#   powershell -ExecutionPolicy Bypass -File install-windows-service.ps1 `
#       -ServerAddr http://<server>:50051 -Token <token>
#
# 卸载：
#   nssm stop helmagent; nssm remove helmagent confirm

param(
    [string]$AgentId = $env:COMPUTERNAME,
    [string]$ServerAddr = "http://127.0.0.1:50051",
    [string]$Token = "dev-token-change-me",
    [string]$Bin = "C:\Program Files\helm-agent\helm-agent.exe",
    [string]$LogDir = "C:\ProgramData\helm-agent\logs",
    [string]$Nssm = "nssm"
)

$ErrorActionPreference = "Stop"
$svcName = "helmagent"

New-Item -ItemType Directory -Force -Path (Split-Path $Bin) | Out-Null
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

& $Nssm install $svcName $Bin `
    "--agent-id" $AgentId `
    "--server-addr" $ServerAddr `
    "--token" $Token `
    "--log-dir" $LogDir
if ($LASTEXITCODE -ne 0) { throw "nssm install failed" }

& $Nssm set $svcName AppStdout "$LogDir\helm-agent.stdout.log"
& $Nssm set $svcName AppStderr "$LogDir\helm-agent.stderr.log"
& $Nssm set $svcName Start SERVICE_AUTO_START
& $Nssm start $svcName

Write-Host "installed and started Windows service '$svcName'"
