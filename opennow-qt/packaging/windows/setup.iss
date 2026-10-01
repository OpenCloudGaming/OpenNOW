; First-install copier for the portable OpenNOW tree.
; This is not the updater. It does not write a Windows Installer
; ProductCode, UpgradeCode, or InstallLocation.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef Arch
  #define Arch "x64"
#endif
#ifndef Payload
  #error Compile with /DPayload pointing at the portable ZIP layout
#endif

#define AppName "OpenNOW"
#define AppExeName "OpenNOW.exe"
#define AppPublisher "OpenCloudGaming"
#define StableUpgradeCode "{6E81F7AE-B19D-4E87-A94A-2B2F01EBF762}"
#define NightlyUpgradeCode "{9661F4F8-656C-4B64-9035-01B04F4822B1}"
#define SupporterUpgradeCode "{B3AF8A40-5F44-445A-AD99-CECE00593601}"

[Setup]
AppId={{7E2A9C14-6B0D-4E58-9F31-0C8A5D2B6E17}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://github.com/OpenCloudGaming/OpenNOW
AppSupportURL=https://github.com/OpenCloudGaming/OpenNOW
DefaultDirName={localappdata}\OpenNOW
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputBaseFilename=OpenNOW-setup
OutputDir=output
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed={#Arch}
ArchitecturesInstallIn64BitMode={#Arch}
UninstallDisplayIcon={app}\bin\{#AppExeName}
SetupIconFile=..\icons\OpenNOW.ico
LicenseFile=..\..\..\LICENSE
ChangesAssociations=no
ChangesEnvironment=no
CloseApplications=yes
UsedUserAreasWarning=no

[Files]
Source: "{#Payload}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: checkedonce

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\bin\{#AppExeName}"; WorkingDir: "{app}\bin"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\bin\{#AppExeName}"; WorkingDir: "{app}\bin"; Tasks: desktopicon

[Run]
Filename: "{app}\bin\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Remove the local tree and leave %APPDATA%\OpenNOW alone.
Type: filesandordirs; Name: "{app}"

[Code]
function MsiEnumRelatedProducts(UpgradeCode: String; Reserved: Cardinal; Index: Integer; ProductCode: String): Integer;
external 'MsiEnumRelatedProductsW@msi.dll stdcall';

function RegisteredProduct(UpgradeCode: String): String;
var
  ProductCode: String;
begin
  SetLength(ProductCode, 38);
  if MsiEnumRelatedProducts(UpgradeCode, 0, 0, ProductCode) = 0 then
    Result := ProductCode
  else
    Result := '';
end;

function RemoveRegisteredProduct(ProductCode: String): Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  if ProductCode = '' then
    exit;
  if Exec(ExpandConstant('{sys}\msiexec.exe'), '/x ' + ProductCode + ' /passive /norestart', '', SW_SHOW, ewWaitUntilTerminated, ResultCode) and ((ResultCode = 0) or (ResultCode = 1605) or (ResultCode = 3010)) then
    exit;
  MsgBox('Windows Installer could not remove the registered OpenNOW product. Install setup.exe again after that product is uninstalled.', mbError, MB_OK);
  Result := False;
end;

function InitializeSetup(): Boolean;
var
  Products: TArrayOfString;
  Index: Integer;
begin
  Result := True;
  SetArrayLength(Products, 3);
  Products[0] := RegisteredProduct('{#StableUpgradeCode}');
  Products[1] := RegisteredProduct('{#NightlyUpgradeCode}');
  Products[2] := RegisteredProduct('{#SupporterUpgradeCode}');
  for Index := 0 to GetArrayLength(Products) - 1 do
  begin
    if Products[Index] <> '' then
    begin
      if MsgBox('OpenNOW is already registered with Windows Installer. Uninstall that copy before installing this one so the two copies do not both claim updates?', mbConfirmation, MB_YESNO) = IDYES then
        Result := RemoveRegisteredProduct(Products[Index])
      else
        Result := True;
      if not Result then
        exit;
    end;
  end;
end;
