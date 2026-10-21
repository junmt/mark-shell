; アンインストール時に、アプリが登録したエクスプローラーの右クリックメニュー（HKCU）を消す
!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegKey HKCU "Software\Classes\Directory\shell\mark-shell"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\mark-shell"
  DeleteRegKey HKCU "Software\Classes\*\shell\mark-shell"
!macroend
