Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "StrFunc.nsh"
${UnStrLoc}

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\openOMSI"

Name "openOMSI"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\openOMSI"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "openOMSI ${VERSION}"

!define MUI_ICON "assets\icons\app\openomsi.ico"
!define MUI_UNICON "assets\icons\app\openomsi.ico"
!define MUI_BGCOLOR 121212
!define MUI_TEXTCOLOR EDEDED
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "assets\icons\app\installer-header.bmp"
!define MUI_HEADERIMAGE_UNBITMAP "assets\icons\app\installer-header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "assets\icons\app\installer-sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "assets\icons\app\installer-sidebar.bmp"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\openomsi.exe"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "French"
!insertmacro MUI_LANGUAGE "German"
!insertmacro MUI_LANGUAGE "Spanish"
!insertmacro MUI_LANGUAGE "Catalan"
!insertmacro MUI_LANGUAGE "Italian"
!insertmacro MUI_LANGUAGE "Dutch"
!insertmacro MUI_LANGUAGE "Swedish"
!insertmacro MUI_LANGUAGE "Polish"
!insertmacro MUI_LANGUAGE "Czech"
!insertmacro MUI_LANGUAGE "Hungarian"
!insertmacro MUI_LANGUAGE "PortugueseBR"
!insertmacro MUI_LANGUAGE "Portuguese"
!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "Ukrainian"
!insertmacro MUI_LANGUAGE "Belarusian"
!insertmacro MUI_LANGUAGE "SerbianLatin"
!insertmacro MUI_LANGUAGE "Turkish"
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "TradChinese"
!insertmacro MUI_LANGUAGE "Japanese"
!insertmacro MUI_LANGUAGE "Korean"
!insertmacro MUI_LANGUAGE "Hindi"
!insertmacro MUI_LANGUAGE "Vietnamese"
!insertmacro MUI_LANGUAGE "Thai"
!insertmacro MUI_LANGUAGE "Malay"
!insertmacro MUI_LANGUAGE "Indonesian"

Section
  SetOutPath "$INSTDIR"
  File "${DIST}\*.*"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  CreateShortcut "$SMPROGRAMS\openOMSI.lnk" "$INSTDIR\openomsi.exe"
  CreateShortcut "$DESKTOP\openOMSI.lnk" "$INSTDIR\openomsi.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "openOMSI"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\openomsi.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "openOMSI"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/openOMSI-org/openOMSI"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  !include "${UNINSTALL_LIST}"
  ClearErrors
  FileOpen $0 "$INSTDIR\.openomsi-files" r
  IfErrors manifest_done
  manifest_next:
    ClearErrors
    FileRead $0 $1
    IfErrors manifest_end
  manifest_trim:
    StrCpy $2 $1 1 -1
    StrCmp $2 "$\n" +2
    StrCmp $2 "$\r" 0 +3
    StrCpy $1 $1 -1
    Goto manifest_trim
    StrCmp $1 "" manifest_next
    StrCmp $1 "." manifest_next
    StrCmp $1 ".." manifest_next
    ${UnStrLoc} $2 $1 "\" ">"
    StrCmp $2 "" 0 manifest_next
    ${UnStrLoc} $2 $1 "/" ">"
    StrCmp $2 "" 0 manifest_next
    Delete "$INSTDIR\$1"
    Goto manifest_next
  manifest_end:
  FileClose $0
  manifest_done:
  Delete "$INSTDIR\*.old-update"
  Delete "$INSTDIR\*.bad-update"
  Delete "$INSTDIR\.openomsi-files"
  Delete "$INSTDIR\.openomsi-previous"
  RMDir /r "$INSTDIR\.openomsi-update"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\openOMSI.lnk"
  Delete "$DESKTOP\openOMSI.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
