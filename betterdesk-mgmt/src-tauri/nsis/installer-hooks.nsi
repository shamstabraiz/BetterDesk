; Yomie NSIS Installer Hooks
; Adds server configuration during installation, autostart registry entry,
; and Windows Firewall rules for required ports.

!include "nsDialogs.nsh"
!include "LogicLib.nsh"

Var Dialog
Var ServerUrlLabel
Var ServerUrlInput
Var ServerUrl
Var ServerKeyLabel
Var ServerKeyInput
Var ServerKey
Var InfoLabel

; ---------------------------------------------------------------------------
;  Custom page — Server Configuration
; ---------------------------------------------------------------------------

Function custom_page_server_config
    nsDialogs::Create 1018
    Pop $Dialog
    ${If} $Dialog == error
        Abort
    ${EndIf}

    ; Title label
    ${NSD_CreateLabel} 0 0 100% 18u "Server Configuration"
    Pop $InfoLabel
    CreateFont $0 "Segoe UI" 11 700
    SendMessage $InfoLabel ${WM_SETFONT} $0 0

    ; Description
    ${NSD_CreateLabel} 0 22u 100% 24u "Enter your Yomie server address. You can change this later in Settings."
    Pop $0

    ; Server URL
    ${NSD_CreateLabel} 0 54u 100% 12u "Server Address (e.g. myserver.example.com or 192.168.1.100):"
    Pop $ServerUrlLabel

    ${NSD_CreateText} 0 68u 100% 14u ""
    Pop $ServerUrlInput

    ; Server Key
    ${NSD_CreateLabel} 0 92u 100% 12u "Server Key (optional — leave blank for auto-detect):"
    Pop $ServerKeyLabel

    ${NSD_CreateText} 0 106u 100% 14u ""
    Pop $ServerKeyInput

    ; Info note
    ${NSD_CreateLabel} 0 130u 100% 24u "If you don't know these values, leave them empty. You can configure the client after installation from the Settings panel."
    Pop $0
    SetCtlColors $0 0x666666 transparent

    nsDialogs::Show
FunctionEnd

Function custom_page_server_config_leave
    ; Read values from inputs
    ${NSD_GetText} $ServerUrlInput $ServerUrl
    ${NSD_GetText} $ServerKeyInput $ServerKey
FunctionEnd

; ---------------------------------------------------------------------------
;  Post-install — Write config + autostart + firewall
; ---------------------------------------------------------------------------

Function custom_page_after_install
    ; 1. Write config.json if user provided a server URL
    StrCmp $ServerUrl "" skip_config

    SetShellVarContext current
    CreateDirectory "$APPDATA\Yomie\config"

    FileOpen $0 "$APPDATA\Yomie\config\config.json" w
    FileWrite $0 '{"server_address":"$ServerUrl:21116","server_key":"$ServerKey","console_url":"http://$ServerUrl:5000","native_protocol":true}'
    FileClose $0

    skip_config:

    ; 2. Create desktop shortcut
    SetShellVarContext current
    CreateShortCut "$DESKTOP\Yomie MGMT.lnk" "$INSTDIR\yomie-mgmt.exe" "" "$INSTDIR\yomie-mgmt.exe" 0

    ; 3. Add autostart registry entry (current user — no admin required)
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" \
        "Yomie MGMT" '"$INSTDIR\yomie-mgmt.exe" --autostart'

    ; 4. Add Windows Firewall rules (requires admin — installer runs elevated)
    ; Signal server TCP
    nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Yomie Signal TCP" dir=in action=allow protocol=TCP localport=21116 enable=yes'
    ; Signal server UDP
    nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Yomie Signal UDP" dir=in action=allow protocol=UDP localport=21116 enable=yes'
    ; Relay server
    nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Yomie Relay" dir=in action=allow protocol=TCP localport=21117 enable=yes'
    ; WebSocket signal
    nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Yomie WS Signal" dir=in action=allow protocol=TCP localport=21118 enable=yes'
    ; WebSocket relay
    nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Yomie WS Relay" dir=in action=allow protocol=TCP localport=21119 enable=yes'
    ; Allow the exe outbound
    nsExec::ExecToLog 'netsh advfirewall firewall add rule name="Yomie MGMT" dir=out action=allow program="$INSTDIR\yomie-mgmt.exe" enable=yes'

FunctionEnd

; ---------------------------------------------------------------------------
;  Uninstall — Clean up shortcuts + firewall + autostart
; ---------------------------------------------------------------------------

Function un.custom_page_after_install
    ; Remove desktop shortcut
    SetShellVarContext current
    Delete "$DESKTOP\Yomie MGMT.lnk"

    ; Remove autostart registry
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Yomie MGMT"

    ; Remove firewall rules
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Yomie Signal TCP"'
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Yomie Signal UDP"'
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Yomie Relay"'
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Yomie WS Signal"'
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Yomie WS Relay"'
    nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="Yomie MGMT"'
FunctionEnd
