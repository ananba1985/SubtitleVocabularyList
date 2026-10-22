; Register only Open With choices. Never write extension defaults or UserChoice.
!macro SVL_OPEN_WITH_EXTENSION EXT
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\SupportedTypes" ".${EXT}" ""
  WriteRegStr SHCTX "Software\Classes\.${EXT}\OpenWithProgids" "${BUNDLEID}.Video" ""
!macroend

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe" "FriendlyAppName" "${PRODUCTNAME}"
  WriteRegStr SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe\shell\open\command" "" '$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\"'
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.Video" "" "${PRODUCTNAME} Video"
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.Video\DefaultIcon" "" '$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0'
  WriteRegStr SHCTX "Software\Classes\${BUNDLEID}.Video\shell\open\command" "" '$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\"'
  !insertmacro SVL_OPEN_WITH_EXTENSION mkv
  !insertmacro SVL_OPEN_WITH_EXTENSION mp4
  !insertmacro SVL_OPEN_WITH_EXTENSION avi
  !insertmacro SVL_OPEN_WITH_EXTENSION mov
  !insertmacro SVL_OPEN_WITH_EXTENSION webm
  !insertmacro SVL_OPEN_WITH_EXTENSION m4v
  !insertmacro SVL_OPEN_WITH_EXTENSION ts
  !insertmacro SVL_OPEN_WITH_EXTENSION mts
  !insertmacro SVL_OPEN_WITH_EXTENSION m2ts
  !insertmacro SVL_OPEN_WITH_EXTENSION wmv
  !insertmacro SVL_OPEN_WITH_EXTENSION flv
  System::Call "shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue SHCTX "Software\Classes\.mkv\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.mp4\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.avi\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.mov\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.webm\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.m4v\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.ts\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.mts\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.m2ts\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.wmv\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegValue SHCTX "Software\Classes\.flv\OpenWithProgids" "${BUNDLEID}.Video"
  DeleteRegKey SHCTX "Software\Classes\${BUNDLEID}.Video"
  DeleteRegKey SHCTX "Software\Classes\Applications\${MAINBINARYNAME}.exe"
  System::Call "shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)"
!macroend
