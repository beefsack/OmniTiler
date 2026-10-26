# Unified lifecycle/service/ABI validation: exactly one exported KWin
# effect plugin hosts the active border, group overlay, and the
# authoritative gesture lifecycle (the retired drag-oracle verdict route is
# gone; the core GestureIngress lane alone owns preview/drop/resize/hover).
# Maximize observation seeds directly from the native committed
# maximizeMode(); no script handoff. No second effect, factory, or metadata.
#
# Required inputs:
#   SOURCE_FILE - activewindowborder.cpp
#   HEADER_FILE - activewindowborder.h
#   CMAKE_FILE  - CMakeLists.txt

if(NOT DEFINED SOURCE_FILE OR NOT DEFINED HEADER_FILE OR NOT DEFINED CMAKE_FILE)
    message(FATAL_ERROR "usage: cmake -DSOURCE_FILE=<file> -DHEADER_FILE=<file> -DCMAKE_FILE=<file> -P validate-unified-lifecycle.cmake")
endif()

file(READ "${SOURCE_FILE}" IMPL)
file(READ "${HEADER_FILE}" HEADER)
file(READ "${CMAKE_FILE}" CMAKE)

# Exactly one effect factory, bound to the effect metadata.
string(REGEX MATCHALL "KWIN_EFFECT_FACTORY" FACTORY_MATCHES "${IMPL}")
list(LENGTH FACTORY_MATCHES FACTORY_COUNT)
if(NOT FACTORY_COUNT EQUAL 1)
    message(FATAL_ERROR "unified lifecycle validation failed: expected exactly one KWIN_EFFECT_FACTORY, found ${FACTORY_COUNT}")
endif()
foreach(FACTORY_PART "ActiveWindowBorderEffect" "metadata.json")
    string(FIND "${IMPL}" "${FACTORY_PART}" FACTORY_PART_POS)
    if(FACTORY_PART_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: '${FACTORY_PART}' missing from survivor factory")
    endif()
endforeach()
foreach(SECOND_PLUGIN "DragOracleEffect" "dragoracle-metadata.json" "omnitiler-drag-oracle")
    string(FIND "${IMPL}" "${SECOND_PLUGIN}" SECOND_IMPL_POS)
    if(NOT SECOND_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: second-plugin residue '${SECOND_PLUGIN}' must not exist in survivor implementation")
    endif()
    string(FIND "${HEADER}" "${SECOND_PLUGIN}" SECOND_HEADER_POS)
    if(NOT SECOND_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: second-plugin residue '${SECOND_PLUGIN}' must not exist in survivor header")
    endif()
endforeach()

# Exactly one D-Bus service/object pair per endpoint, each unregistered:
# the ActiveBorder presentation endpoint plus the authoritative gesture
# lifecycle endpoint (the retired DragOracle verdict endpoint is gone).
foreach(ENDPOINT "com.omnitiler.ActiveBorder" "/com/omnitiler/ActiveBorder" "com.omnitiler.Gesture" "/com/omnitiler/Gesture")
    string(FIND "${IMPL}" "${ENDPOINT}" ENDPOINT_POS)
    if(ENDPOINT_POS EQUAL -1)
        message(FATAL_ERROR "unified service validation failed: '${ENDPOINT}' not found")
    endif()
endforeach()
foreach(UNREG "unregisterObject" "unregisterService")
    string(REGEX MATCHALL "${UNREG}" UNREG_MATCHES "${IMPL}")
    list(LENGTH UNREG_MATCHES UNREG_COUNT)
    if(UNREG_COUNT LESS 2)
        message(FATAL_ERROR "unified service validation failed: expected independent '${UNREG}' for both endpoints, found ${UNREG_COUNT}")
    endif()
endforeach()

# Authoritative gesture lifecycle: the effect mints the immutable native
# identity, freezes start facts, and resolves the phase through the
# effect-owned source. The retired oracle verdict route (LastVerdict,
# press spy, FFI record calls) must not survive in the implementation.
foreach(GESTURE_SYMBOL "ensureGestureEndpointRegistered" "GestureFactsSource" "gestureStarted" "gestureFinished" "gestureAborted")
    string(FIND "${IMPL}" "${GESTURE_SYMBOL}" GESTURE_POS)
    if(GESTURE_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: '${GESTURE_SYMBOL}' not found in survivor implementation")
    endif()
endforeach()
foreach(RETIRED_ORACLE "LastVerdict" "com.omnitiler.DragOracle" "drag_oracle_record" "drag_oracle_last_copy" "attachOracleWindow" "forgetOracleWindow" "onOracleDragStart" "onOracleDragFinish" "OraclePress" "m_oracle")
    string(FIND "${IMPL}" "${RETIRED_ORACLE}" RETIRED_IMPL_POS)
    if(NOT RETIRED_IMPL_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: retired oracle residue '${RETIRED_ORACLE}' must not remain in survivor implementation")
    endif()
    string(FIND "${HEADER}" "${RETIRED_ORACLE}" RETIRED_HEADER_POS)
    if(NOT RETIRED_HEADER_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: retired oracle residue '${RETIRED_ORACLE}' must not remain in survivor header")
    endif()
endforeach()
foreach(HEADER_TOKEN "m_gestureAttached" "m_gestureWinIds" "attachGestureWindow" "forgetGestureWindow" "onGestureDragStart" "onGestureDragFinish")
    string(FIND "${HEADER}" "${HEADER_TOKEN}" HEADER_TOKEN_POS)
    if(HEADER_TOKEN_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: '${HEADER_TOKEN}' missing from survivor header")
    endif()
endforeach()

# Gesture observation plus the group-underlay lowest-stacked anchor share
# the existing lifecycle: two stacking-order passes (construction seed plus
# anchor choice) plus single windowAdded/closed/deleted connections that
# also drive the gesture attach/forget state through the shared lambdas.
# Count exact stackingOrder() calls so the stackingOrderChanged signal and
# comments do not inflate the count.
string(REGEX MATCHALL "stackingOrder\\(\\)" STACK_MATCHES "${IMPL}")
list(LENGTH STACK_MATCHES STACK_COUNT)
if(NOT STACK_COUNT EQUAL 2)
    message(FATAL_ERROR "unified lifecycle validation failed: expected construction seed plus group anchor stackingOrder() passes, found ${STACK_COUNT}")
endif()
string(FIND "${IMPL}" "stackingOrderChanged" STACK_CHANGED_POS)
if(STACK_CHANGED_POS EQUAL -1)
    message(FATAL_ERROR "unified lifecycle validation failed: stackingOrderChanged re-anchor missing")
endif()
foreach(HOOK "attachGestureWindow" "forgetGestureWindow" "onGestureDragStart" "onGestureDragFinish" "windowStartUserMovedResized" "windowFinishUserMovedResized" "moveResizeGeometry")
    string(FIND "${IMPL}" "${HOOK}" HOOK_POS)
    if(HOOK_POS EQUAL -1)
        message(FATAL_ERROR "unified lifecycle validation failed: '${HOOK}' not found")
    endif()
endforeach()
foreach(WINDOW_HOOK "windowAdded" "windowClosed" "windowDeleted")
    string(REGEX MATCHALL "EffectsHandler::${WINDOW_HOOK}" WINDOW_HOOK_MATCHES "${IMPL}")
    list(LENGTH WINDOW_HOOK_MATCHES WINDOW_HOOK_COUNT)
    if(NOT WINDOW_HOOK_COUNT EQUAL 1)
        message(FATAL_ERROR "unified lifecycle validation failed: expected exactly one '${WINDOW_HOOK}' hookup shared with the gesture lifecycle, found ${WINDOW_HOOK_COUNT}")
    endif()
endforeach()
# The gesture start-geometry helper reads moveResizeGeometry only (no
# geometry fallback beside it); the border's own frameGeometry rendering
# path above is unrelated and untouched.
string(FIND "${IMPL}" "gestureMoveResizeRect" GESTURE_HELPER_POS)
if(GESTURE_HELPER_POS EQUAL -1)
    message(FATAL_ERROR "unified lifecycle validation failed: gestureMoveResizeRect helper missing")
endif()

# Rendering: the preserved active outline, the filled group underlay below
# every member, the drag preview, plus the pass-through paint.
foreach(RENDER_TOKEN "m_borderItem" "m_groupItem" "m_dragPreviewItem" "paintScreen" "updateGroupAnchorAndGeometry" "stackingOrderChanged")
    string(FIND "${IMPL}" "${RENDER_TOKEN}" RENDER_POS)
    if(RENDER_POS EQUAL -1)
        message(FATAL_ERROR "unified rendering validation failed: '${RENDER_TOKEN}' not found")
    endif()
endforeach()
string(REGEX MATCHALL "OutlinedBorderItem" OUTLINE_MATCHES "${HEADER}")
list(LENGTH OUTLINE_MATCHES OUTLINE_COUNT)
if(NOT OUTLINE_COUNT EQUAL 1)
    message(FATAL_ERROR "unified rendering validation failed: expected exactly one OutlinedBorderItem member, found ${OUTLINE_COUNT}")
endif()

# CMake builds only the effect plus the KCM: no second plugin target,
# factory source, standalone metadata, or validation script.
foreach(LEGACY_TOKEN "dragoracle.h" "dragoracle.cpp" "dragoracle-metadata.json" "validate-dragoracle.cmake" "native-effect-drag-oracle-validation" "EXPECTED_PLUGIN_ID=omnitiler-drag-oracle" "kcoreaddons_add_plugin(omnitiler-drag-oracle")
    string(FIND "${CMAKE}" "${LEGACY_TOKEN}" LEGACY_POS)
    if(NOT LEGACY_POS EQUAL -1)
        message(FATAL_ERROR "unified build validation failed: legacy token '${LEGACY_TOKEN}' must not remain in CMakeLists.txt")
    endif()
endforeach()
foreach(SURVIVOR_TOKEN "omnitiler-active-border" "Qt6::DBus" "presentation_consumer.h" "native-effect-presentation-consumer" "native-effect-unified-lifecycle")
    string(FIND "${CMAKE}" "${SURVIVOR_TOKEN}" SURVIVOR_POS)
    if(SURVIVOR_POS EQUAL -1)
        message(FATAL_ERROR "unified build validation failed: '${SURVIVOR_TOKEN}' missing from CMakeLists.txt")
    endif()
endforeach()

message(STATUS "unified lifecycle/service/ABI validation passed for ${SOURCE_FILE}")
