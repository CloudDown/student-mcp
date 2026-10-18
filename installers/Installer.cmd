@echo off
title Connecter Moodle a mon assistant
"%~dp0student-mcp.exe" install
if errorlevel 1 (
  echo.
  echo Installation incomplete. Consultez README.md pour obtenir de l'aide.
)
echo.
pause
