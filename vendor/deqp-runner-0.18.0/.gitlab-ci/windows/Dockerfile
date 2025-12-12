# escape=`

FROM mcr.microsoft.com/windows/servercore:ltsc2022

# Make sure any failure in PowerShell scripts is fatal
SHELL ["powershell", "-ExecutionPolicy", "RemoteSigned", "-Command", "$ErrorActionPreference = 'Stop';"]
ENV ErrorActionPreference='Stop'

COPY deqp_runner_deps_vs2019.ps1 C:\
RUN C:\deqp_runner_deps_vs2019.ps1

COPY deqp_runner_deps_build.ps1 C:\
RUN C:\deqp_runner_deps_build.ps1
