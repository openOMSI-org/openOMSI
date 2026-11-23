Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "MUI2.nsh"
!include "StrFunc.nsh"
${UnStrLoc}

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\openBus"

Name "openBus"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\openBus"
InstallDirRegKey HKCU "${UNINSTALL_KEY}" "InstallLocation"
BrandingText "openBus ${VERSION}"

!define MUI_ICON "assets\icons\app\openbus.ico"
!define MUI_UNICON "assets\icons\app\openbus.ico"
!define MUI_BGCOLOR 121212
!define MUI_TEXTCOLOR EDEDED
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT
!define MUI_HEADERIMAGE_BITMAP "assets\icons\app\installer-header.bmp"
!define MUI_HEADERIMAGE_UNBITMAP "assets\icons\app\installer-header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "assets\icons\app\installer-sidebar.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "assets\icons\app\installer-sidebar.bmp"
!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\openbus.exe"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_PAGE_CUSTOMFUNCTION_SHOW FinishShow
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
  CreateShortcut "$SMPROGRAMS\openBus.lnk" "$INSTDIR\openbus.exe"
  CreateShortcut "$DESKTOP\openBus.lnk" "$INSTDIR\openbus.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "openBus"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\openbus.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "openBus"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/rafaelkaiz/openBus"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Function FinishShow
  System::Call 'uxtheme::SetWindowTheme(p $mui.FinishPage.Run, w " ", w " ")'
  SetCtlColors $mui.FinishPage.Run EDEDED 121212
FunctionEnd

Section "Uninstall"
  !include "${UNINSTALL_LIST}"
  ClearErrors
  FileOpen $0 "$INSTDIR\.openbus-files" r
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
  ; (what the launcher's optional NVIDIA DLSS installer put beside the game)
  Delete "$INSTDIR\sl.interposer.dll"
  Delete "$INSTDIR\sl.common.dll"
  Delete "$INSTDIR\sl.dlss.dll"
  Delete "$INSTDIR\nvngx_dlss.dll"
  Delete "$INSTDIR\nvngx_dlss.license.txt"
  Delete "$INSTDIR\NVIDIA-Streamline-LICENSE.txt"
  Delete "$INSTDIR\NVIDIA-Streamline-NOTICE.txt"
  Delete "$INSTDIR\*.old-update"
  Delete "$INSTDIR\*.bad-update"
  Delete "$INSTDIR\.openbus-files"
  Delete "$INSTDIR\.openbus-previous"
  RMDir /r "$INSTDIR\.openbus-update"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\openBus.lnk"
  Delete "$DESKTOP\openBus.lnk"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
