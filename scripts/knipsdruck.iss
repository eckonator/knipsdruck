; SPDX-License-Identifier: MIT OR Apache-2.0
; Windows-Installer für Knipsdruck (Inno Setup 6).
;
; Aufruf: iscc /DAppVersion=0.1.0 [/DStageDir=..\dist\windows] [/DOutputDir=..\dist] scripts\knipsdruck.iss
;   StageDir: Knipsdruck.exe, knipsdruck-cli.exe, README.md und licenses\ (Pfade relativ zu diesem Skript)
;
; Standard ist die Installation nur für den aktuellen Benutzer nach %LOCALAPPDATA%\Programs\Knipsdruck,
; ohne Administratorrechte. Im Auswahldialog lässt sich auch für alle Benutzer installieren (Programme);
; nur dann wird eine Firewall-Regel für private Netze eingetragen, damit das Handy die App erreicht.
; Ohne Regel fragt Windows beim ersten Empfang selbst nach.
;
; Update aus der App: Sie startet den neuen Installer mit
;   /SILENT /SUPPRESSMSGBOXES /NORESTART /RELAUNCH
; und beendet sich. Setup schließt eine noch laufende Knipsdruck.exe (CloseApplications) und startet sie
; wegen /RELAUNCH danach wieder. Ohne /RELAUNCH bleibt die App nach einer stillen Installation zu;
; bei normaler Installation gibt es stattdessen das Häkchen „Knipsdruck starten“ auf der letzten Seite.
;
; Als UTF-8 mit BOM speichern, sonst liest Inno Setup Umlaute und © falsch.

#ifndef AppVersion
  #error AppVersion fehlt: iscc /DAppVersion=x.y.z
#endif
#ifndef StageDir
  #define StageDir "..\dist\windows"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

[Setup]
; Feste Kennung, nie ändern: daran erkennt Setup eine vorhandene Installation
AppId={{3EB9F952-F8DE-4C3E-9389-0D27104277AC}
AppName=Knipsdruck
AppVersion={#AppVersion}
AppVerName=Knipsdruck {#AppVersion}
AppPublisher=Markus Eckert
AppPublisherURL=https://github.com/eckonator/knipsdruck
AppSupportURL=https://github.com/eckonator/knipsdruck/issues
AppUpdatesURL=https://github.com/eckonator/knipsdruck/releases
AppCopyright=© 2026 Markus Eckert · MIT OR Apache-2.0
VersionInfoVersion={#AppVersion}
VersionInfoProductName=Knipsdruck
; {autopf}: %LOCALAPPDATA%\Programs für einen Benutzer, sonst C:\Program Files
DefaultDirName={autopf}\Knipsdruck
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern
ShowLanguageDialog=auto
SetupIconFile=..\assets\knipsdruck.ico
UninstallDisplayIcon={app}\Knipsdruck.exe
UninstallDisplayName=Knipsdruck
CloseApplications=yes
RestartApplications=no
OutputDir={#OutputDir}
OutputBaseFilename=Knipsdruck-{#AppVersion}-Windows-Setup
Compression=lzma2/max
SolidCompression=yes

[Languages]
; Erste Sprache gilt, wenn Windows weder Deutsch noch Englisch ist
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"

[CustomMessages]
en.FirewallRule=Allowing phone uploads in Windows Firewall...
de.FirewallRule=Handy-Empfang in der Windows-Firewall freigeben...

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#StageDir}\Knipsdruck.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\knipsdruck-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#StageDir}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion recursesubdirs

[Icons]
Name: "{autoprograms}\Knipsdruck"; Filename: "{app}\Knipsdruck.exe"
Name: "{autodesktop}\Knipsdruck"; Filename: "{app}\Knipsdruck.exe"; Tasks: desktopicon

[Run]
; Firewall nur bei Installation für alle Benutzer (braucht Administratorrechte). Erst eine alte Regel
; löschen, damit ein Update keine doppelte anlegt; schlägt fehl, wenn es keine gibt, das ist egal.
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Knipsdruck"" program=""{app}\Knipsdruck.exe"""; Flags: runhidden; Check: IsAdminInstallMode
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall add rule name=""Knipsdruck"" dir=in action=allow program=""{app}\Knipsdruck.exe"" profile=private"; Flags: runhidden; StatusMsg: "{cm:FirewallRule}"; Check: IsAdminInstallMode
Filename: "{app}\Knipsdruck.exe"; Description: "{cm:LaunchProgram,Knipsdruck}"; Flags: nowait postinstall skipifsilent
; Stilles Update aus der App heraus (siehe oben), als der Benutzer, der Setup gestartet hat
Filename: "{app}\Knipsdruck.exe"; Flags: nowait runasoriginaluser; Check: ShouldRelaunch

[UninstallRun]
Filename: "{sys}\netsh.exe"; Parameters: "advfirewall firewall delete rule name=""Knipsdruck"" program=""{app}\Knipsdruck.exe"""; Flags: runhidden; RunOnceId: "FirewallRule"; Check: IsAdminInstallMode

[Code]
// /RELAUNCH zählt nur bei stiller Installation; sonst entscheidet das Häkchen auf der letzten Seite
function ShouldRelaunch: Boolean;
var
  I: Integer;
begin
  Result := False;
  if not WizardSilent then
    exit;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/RELAUNCH') = 0 then
      Result := True;
end;
