; アンインストール時に、アプリが登録したエクスプローラーの右クリックメニュー（HKCU）を消す
!macro NSIS_HOOK_PREUNINSTALL
  ; コマンド mark-shell（%LOCALAPPDATA%mark-shellin と PATH への登録）を外す
  ExecWait '"$INSTDIRmark-shell.exe" --unregister-cli'
  DeleteRegKey HKCU "Software\Classes\Directory\shell\mark-shell"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\mark-shell"
  DeleteRegKey HKCU "Software\Classes\*\shell\mark-shell"
!macroend
